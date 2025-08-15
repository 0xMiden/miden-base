use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::errors::TransactionKernelError;
use miden_lib::transaction::TransactionEvent;
use miden_objects::account::{AccountDelta, PartialAccount};
use miden_objects::assembly::debuginfo::Location;
use miden_objects::assembly::{DefaultSourceManager, SourceFile, SourceManager, SourceSpan};
use miden_objects::asset::FungibleAsset;
use miden_objects::block::FeeParameters;
use miden_objects::transaction::{InputNote, InputNotes, OutputNote};
use miden_objects::{Felt, Hasher, Word};
use vm_processor::{
    AdviceMutation,
    AsyncHost,
    BaseHost,
    EventError,
    FutureMaybeSend,
    MastForest,
    MastForestStore,
    ProcessState,
};

use crate::AccountProcedureIndexMap;
use crate::auth::{SigningInputs, TransactionAuthenticator};
use crate::host::{
    ScriptMastForestStore,
    TransactionBaseHost,
    TransactionEventData,
    TransactionEventHandling,
    TransactionProgress,
};

/// The transaction executor host is responsible for handling [`FutureMaybeSend`] requests made by
/// the transaction kernel during execution. In particular, it responds to signature generation
/// requests by forwarding the request to the contained [`TransactionAuthenticator`].
///
/// Transaction hosts are created on a per-transaction basis. That is, a transaction host is meant
/// to support execution of a single transaction and is discarded after the transaction finishes
/// execution.
pub struct TransactionExecutorHost<'store, 'auth, STORE, AUTH>
where
    STORE: MastForestStore,
    AUTH: TransactionAuthenticator,
{
    /// The underlying base transaction host.
    base_host: TransactionBaseHost<'store, STORE>,

    /// Serves signature generation requests from the transaction runtime for signatures which are
    /// not present in the `generated_signatures` field.
    authenticator: Option<&'auth AUTH>,

    /// Contains generated signatures (as a message |-> signature map) required for transaction
    /// execution. Once a signature was created for a given message, it is inserted into this map.
    /// After transaction execution, these can be inserted into the advice inputs to re-execute the
    /// transaction without having to regenerate the signature or requiring access to the
    /// authenticator that produced it.
    generated_signatures: BTreeMap<Word, Vec<Felt>>,

    /// The balance of the native asset in the account at the beginning of transaction execution.
    initial_native_asset: FungibleAsset,
}

impl<'store, 'auth, STORE, AUTH> TransactionExecutorHost<'store, 'auth, STORE, AUTH>
where
    STORE: MastForestStore + Sync,
    AUTH: TransactionAuthenticator + Sync,
{
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`TransactionExecutorHost`] instance from the provided inputs.
    pub fn new(
        account: &PartialAccount,
        input_notes: InputNotes<InputNote>,
        mast_store: &'store STORE,
        scripts_mast_store: ScriptMastForestStore,
        acct_procedure_index_map: AccountProcedureIndexMap,
        authenticator: Option<&'auth AUTH>,
        fee_parameters: &FeeParameters,
    ) -> Self {
        // TODO: Once we have lazy account loading, this should be loaded in on_tx_fee_computed to
        // avoid the use of PartialVault entirely, which in the future, may or may not track
        // all assets in the account at this point. Here we assume it does track _all_ assets of the
        // account.
        let initial_native_asset = {
            let native_asset = FungibleAsset::new(fee_parameters.native_asset_id(), 0)
                .expect("native asset ID should be a valid fungible faucet ID");

            // Map Asset to FungibleAsset.
            // SAFETY: We requested a fungible vault key, so if Some is returned, it should be a
            // fungible asset.
            // A returned error means the vault does not track or does not contain the asset.
            // However, since in practice, the partial vault represents the entire
            // account vault, we can assume the second case.
            // A returned None means the asset isn't tracked.
            // So in both Err and None cases, use the default native_asset with amount 0.
            account
                .vault()
                .get(native_asset.vault_key())
                .map(|asset| asset.map(|asset| asset.unwrap_fungible()).unwrap_or(native_asset))
                .unwrap_or(native_asset)
        };

        let base_host = TransactionBaseHost::new(
            account,
            input_notes,
            mast_store,
            scripts_mast_store,
            acct_procedure_index_map,
        );

        Self {
            base_host,
            authenticator,
            generated_signatures: BTreeMap::new(),
            initial_native_asset,
        }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the `tx_progress` field of this transaction host.
    pub fn tx_progress(&self) -> &TransactionProgress {
        self.base_host.tx_progress()
    }

    // EVENT HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Pushes a signature to the advice stack as a response to the `AuthRequest` event.
    ///
    /// The signature is requested from the host's authenticator.
    pub async fn on_auth_requested(
        &mut self,
        pub_key_hash: Word,
        signing_inputs: SigningInputs,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        let authenticator =
            self.authenticator.ok_or(TransactionKernelError::MissingAuthenticator)?;

        let signature: Vec<Felt> = authenticator
            .get_signature(pub_key_hash, &signing_inputs)
            .await
            .map_err(|err| TransactionKernelError::SignatureGenerationFailed(Box::new(err)))?;

        let signature_key = Hasher::merge(&[pub_key_hash, signing_inputs.to_commitment()]);

        self.generated_signatures.insert(signature_key, signature.clone());

        Ok(vec![AdviceMutation::extend_stack(signature)])
    }

    /// Handles the [`TransactionEvent::EpilogueTxFeeComputed`] and returns an error if the account
    /// cannot pay the fee.
    fn on_tx_fee_computed(
        &self,
        fee_asset: FungibleAsset,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        // Compute the current balance of the native asset in the account based on the initial value
        // and the delta.
        let current_native_asset = {
            let native_asset_amount_delta = self
                .base_host
                .account_delta_tracker()
                .vault_delta()
                .fungible()
                .amount(&self.initial_native_asset.faucet_id())
                .unwrap_or(0);

            // SAFETY: Initial native asset faucet ID should be a fungible faucet and amount should
            // be less than MAX_AMOUNT as checked by the account delta.
            let native_asset_delta = FungibleAsset::new(
                self.initial_native_asset.faucet_id(),
                native_asset_amount_delta.unsigned_abs(),
            )
            .expect("faucet ID and amount should be valid");

            // SAFETY: These computations are essentially the same as the ones executed by the
            // transaction kernel, which should have aborted if they weren't valid.
            if native_asset_amount_delta > 0 {
                self.initial_native_asset
                    .add(native_asset_delta)
                    .expect("transaction kernel should ensure amounts do not exceed MAX_AMOUNT")
            } else {
                self.initial_native_asset
                    .sub(native_asset_delta)
                    .expect("transaction kernel should ensure amount is not negative")
            }
        };

        // Return an error if the balance in the account does not cover the fee.
        if current_native_asset.amount() < fee_asset.amount() {
            return Err(TransactionKernelError::InsufficientFee {
                account_balance: current_native_asset.amount(),
                tx_fee: fee_asset.amount(),
            });
        }

        Ok(Vec::new())
    }

    /// Consumes `self` and returns the account delta, output notes, generated signatures and
    /// transaction progress.
    pub fn into_parts(
        self,
    ) -> (AccountDelta, Vec<OutputNote>, BTreeMap<Word, Vec<Felt>>, TransactionProgress) {
        let (account_delta, output_notes, tx_progress) = self.base_host.into_parts();

        (account_delta, output_notes, self.generated_signatures, tx_progress)
    }
}

// HOST IMPLEMENTATION
// ================================================================================================

impl<STORE, AUTH> BaseHost for TransactionExecutorHost<'_, '_, STORE, AUTH>
where
    STORE: MastForestStore,
    AUTH: TransactionAuthenticator,
{
    fn get_mast_forest(&self, procedure_root: &Word) -> Option<Arc<MastForest>> {
        self.base_host.get_mast_forest(procedure_root)
    }

    fn get_label_and_source_file(
        &self,
        location: &Location,
    ) -> (SourceSpan, Option<Arc<SourceFile>>) {
        // TODO: Replace with proper call to source manager once the host owns it.
        let stub_source_manager = DefaultSourceManager::default();
        let maybe_file = stub_source_manager.get_by_uri(location.uri());
        let span = stub_source_manager.location_to_span(location.clone()).unwrap_or_default();
        (span, maybe_file)
    }
}

impl<STORE, AUTH> AsyncHost for TransactionExecutorHost<'_, '_, STORE, AUTH>
where
    STORE: MastForestStore + Sync,
    AUTH: TransactionAuthenticator + Sync,
{
    fn on_event(
        &mut self,
        process: &ProcessState,
        event_id: u32,
    ) -> impl FutureMaybeSend<Result<Vec<AdviceMutation>, EventError>> {
        // TODO: Eventually, refactor this to let TransactionEvent contain the data directly, which
        // should be cleaner.
        let event_handling_result = TransactionEvent::try_from(event_id)
            .map_err(EventError::from)
            .and_then(|transaction_event| self.base_host.handle_event(process, transaction_event));

        async move {
            let event_handling = event_handling_result?;
            let event_data = match event_handling {
                TransactionEventHandling::Unhandled(event) => event,
                TransactionEventHandling::Handled(mutations) => {
                    return Ok(mutations);
                },
            };

            match event_data {
                TransactionEventData::AuthRequest { pub_key_hash, signing_inputs } => self
                    .on_auth_requested(pub_key_hash, signing_inputs)
                    .await
                    .map_err(EventError::from),
                TransactionEventData::TransactionFeeComputed { fee_asset } => {
                    self.on_tx_fee_computed(fee_asset).map_err(EventError::from)
                },
            }
        }
    }
}

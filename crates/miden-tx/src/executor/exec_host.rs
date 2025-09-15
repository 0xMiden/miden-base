use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::{TransactionAdviceInputs, TransactionEvent};
use miden_objects::account::{
    AccountCode,
    AccountDelta,
    AccountId,
    PartialAccount,
    StorageSlotType,
};
use miden_objects::assembly::debuginfo::Location;
use miden_objects::assembly::{SourceFile, SourceManagerSync, SourceSpan};
use miden_objects::asset::{Asset, AssetWitness, FungibleAsset};
use miden_objects::block::BlockNumber;
use miden_objects::crypto::merkle::SmtProof;
use miden_objects::transaction::{InputNote, InputNotes, OutputNote};
use miden_objects::vm::AdviceMap;
use miden_objects::{Felt, Hasher, Word};
use miden_processor::{
    AdviceMutation,
    AsyncHost,
    BaseHost,
    EventError,
    FutureMaybeSend,
    MastForest,
    ProcessState,
};

use crate::auth::{SigningInputs, TransactionAuthenticator};
use crate::errors::TransactionKernelError;
use crate::host::{
    ScriptMastForestStore,
    TransactionBaseHost,
    TransactionEventData,
    TransactionEventHandling,
    TransactionProgress,
};
use crate::{AccountProcedureIndexMap, DataStore};

// TRANSACTION EXECUTOR HOST
// ================================================================================================

/// The transaction executor host is responsible for handling [`FutureMaybeSend`] requests made by
/// the transaction kernel during execution. In particular, it responds to signature generation
/// requests by forwarding the request to the contained [`TransactionAuthenticator`].
///
/// Transaction hosts are created on a per-transaction basis. That is, a transaction host is meant
/// to support execution of a single transaction and is discarded after the transaction finishes
/// execution.
pub struct TransactionExecutorHost<'store, 'auth, STORE, AUTH>
where
    STORE: DataStore,
    AUTH: TransactionAuthenticator,
{
    /// The underlying base transaction host.
    base_host: TransactionBaseHost<'store, STORE>,

    /// Serves signature generation requests from the transaction runtime for signatures which are
    /// not present in the `generated_signatures` field.
    authenticator: Option<&'auth AUTH>,

    /// The reference block of the transaction.
    ref_block: BlockNumber,

    /// The foreign account code that was lazy loaded during transaction execution.
    ///
    /// This is required for re-executing the transaction, e.g. as part of transaction proving.
    accessed_foreign_account_code: Vec<AccountCode>,

    /// Contains generated signatures (as a message |-> signature map) required for transaction
    /// execution. Once a signature was created for a given message, it is inserted into this map.
    /// After transaction execution, these can be inserted into the advice inputs to re-execute the
    /// transaction without having to regenerate the signature or requiring access to the
    /// authenticator that produced it.
    generated_signatures: BTreeMap<Word, Vec<Felt>>,

    /// The source manager to track source code file span information, improving any MASM related
    /// error messages.
    source_manager: Arc<dyn SourceManagerSync>,
}

impl<'store, 'auth, STORE, AUTH> TransactionExecutorHost<'store, 'auth, STORE, AUTH>
where
    STORE: DataStore + Sync,
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
        ref_block: BlockNumber,
        source_manager: Arc<dyn SourceManagerSync>,
    ) -> Self {
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
            ref_block,
            accessed_foreign_account_code: Vec::new(),
            generated_signatures: BTreeMap::new(),
            source_manager,
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

    /// Handles a request for a foreign account by querying the data store for its account inputs.
    async fn on_foreign_account_requested(
        &mut self,
        foreign_account_id: AccountId,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        let foreign_account_inputs = self
            .base_host
            .store()
            .get_foreign_account_inputs(foreign_account_id, self.ref_block)
            .await
            .map_err(|err| TransactionKernelError::GetForeignAccountInputs {
                foreign_account_id,
                ref_block: self.ref_block,
                source: err,
            })?;

        let mut tx_advice_inputs = TransactionAdviceInputs::default();
        tx_advice_inputs
            .add_foreign_accounts([&foreign_account_inputs])
            .map_err(|err| {
                TransactionKernelError::other_with_source(
                    format!(
                        "failed to construct advice inputs for foreign account {}",
                        foreign_account_inputs.id()
                    ),
                    err,
                )
            })?;

        self.base_host
            .load_foreign_account_code(foreign_account_inputs.code())
            .map_err(|err| {
                TransactionKernelError::other_with_source(
                    format!(
                        "failed to insert account procedures for foreign account {}",
                        foreign_account_inputs.id()
                    ),
                    err,
                )
            })?;

        // Add the foreign account's code to the list of accessed code.
        self.accessed_foreign_account_code.push(foreign_account_inputs.code().clone());

        Ok(tx_advice_inputs.into_advice_mutations().collect())
    }

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
            .map_err(TransactionKernelError::SignatureGenerationFailed)?;

        let signature_key = Hasher::merge(&[pub_key_hash, signing_inputs.to_commitment()]);

        self.generated_signatures.insert(signature_key, signature.clone());

        Ok(vec![AdviceMutation::extend_stack(signature)])
    }

    /// Handles the [`TransactionEvent::EpilogueBeforeTxFeeRemovedFromAccount`] and returns an error
    /// if the account cannot pay the fee.
    async fn on_before_tx_fee_removed_from_account(
        &self,
        fee_asset: FungibleAsset,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        let asset_witness = self
            .base_host
            .store()
            .get_vault_asset_witness(
                self.base_host.initial_account_header().id(),
                self.base_host.initial_account_header().vault_root(),
                fee_asset.vault_key(),
            )
            .await
            .map_err(|err| TransactionKernelError::GetVaultAssetWitness {
                vault_root: self.base_host.initial_account_header().vault_root(),
                vault_key: fee_asset.vault_key(),
                source: err,
            })?;

        // Find fee asset in the witness or default to 0 if it isn't present.
        let initial_fee_asset = asset_witness
            .find(fee_asset.vault_key())
            .and_then(|asset| match asset {
                Asset::Fungible(fungible_asset) => Some(fungible_asset),
                _ => None,
            })
            .unwrap_or(
                FungibleAsset::new(fee_asset.faucet_id(), 0)
                    .expect("fungible asset created from fee asset should be valid"),
            );

        // Compute the current balance of the native asset in the account based on the initial value
        // and the delta.
        let current_fee_asset = {
            let fee_asset_amount_delta = self
                .base_host
                .account_delta_tracker()
                .vault_delta()
                .fungible()
                .amount(&initial_fee_asset.faucet_id())
                .unwrap_or(0);

            // SAFETY: Initial native asset faucet ID should be a fungible faucet and amount should
            // be less than MAX_AMOUNT as checked by the account delta.
            let fee_asset_delta = FungibleAsset::new(
                initial_fee_asset.faucet_id(),
                fee_asset_amount_delta.unsigned_abs(),
            )
            .expect("faucet ID and amount should be valid");

            // SAFETY: These computations are essentially the same as the ones executed by the
            // transaction kernel, which should have aborted if they weren't valid.
            if fee_asset_amount_delta > 0 {
                initial_fee_asset
                    .add(fee_asset_delta)
                    .expect("transaction kernel should ensure amounts do not exceed MAX_AMOUNT")
            } else {
                initial_fee_asset
                    .sub(fee_asset_delta)
                    .expect("transaction kernel should ensure amount is not negative")
            }
        };

        // Return an error if the balance in the account does not cover the fee.
        if current_fee_asset.amount() < fee_asset.amount() {
            return Err(TransactionKernelError::InsufficientFee {
                account_balance: current_fee_asset.amount(),
                tx_fee: fee_asset.amount(),
            });
        }

        Ok(asset_witness_to_advice_mutation(asset_witness))
    }

    /// Handles a request for a storage map witness by querying the data store for a merkle path.
    ///
    /// Note that we request witnesses against the _initial_ map root of the accounts. See also
    /// [`Self::on_account_vault_asset_witness_requested`] for more on this topic.
    async fn on_account_storage_map_witness_requested(
        &self,
        current_account_id: AccountId,
        slot_index: usize,
        current_map_root: Word,
        map_key: Word,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        // For the native account we need to explicitly request the initial map root, while for
        // foreign accounts the current map root is always the initial one.
        let map_root = if current_account_id == self.base_host.initial_account_header().id() {
            // For native accounts, we have to request witnesses against the initial root instead of
            // the _current_ one, since the data store only has witnesses for initial
            // one.
            let (slot_type, slot_value) =
                self.base_host.initial_account_storage_header().slot(slot_index).map_err(
                    |err| {
                        TransactionKernelError::other_with_source(
                            "failed to access storage map in storage header",
                            err,
                        )
                    },
                )?;
            if *slot_type != StorageSlotType::Map {
                return Err(TransactionKernelError::other(format!(
                    "expected map slot type at slot index {slot_index}"
                )));
            }
            *slot_value
        } else {
            current_map_root
        };

        let storage_map_witness = self
            .base_host
            .store()
            .get_storage_map_witness(current_account_id, map_root, map_key)
            .await
            .map_err(|err| TransactionKernelError::GetStorageMapWitness {
                map_root,
                map_key,
                source: err,
            })?;

        // Get the nodes in the proof and insert them into the merkle store.
        let merkle_store_ext =
            AdviceMutation::extend_merkle_store(storage_map_witness.authenticated_nodes());

        let smt_proof = SmtProof::from(storage_map_witness);
        let map_ext = AdviceMutation::extend_map(AdviceMap::from_iter([(
            smt_proof.leaf().hash(),
            smt_proof.leaf().to_elements(),
        )]));

        Ok(vec![merkle_store_ext, map_ext])
    }

    /// Handles a request to an asset witness by querying the data store for a merkle path.
    ///
    /// ## Native Account
    ///
    /// For the native account we always request witnesses for the initial vault root, because the
    /// data store only has the state of the account vault at the beginning of the transaction.
    /// Since the vault root can change as the transaction progresses, this means the witnesses
    /// may become _partially_ or fully outdated. To see why they can only be _partially_ outdated,
    /// consider the following example:
    ///
    /// ```text
    ///      A               A'
    ///     / \             /  \
    ///    B   C    ->    B'    C
    ///   / \  / \       /  \  / \
    ///  D  E F   G     D   E' F  G
    /// ```
    ///
    /// Leaf E was updated to E', in turn updating nodes B and A. If we now request the merkle path
    /// to G against root A (the initial vault root), we'll get nodes F and B. F is a node in the
    /// updated tree, while B is not. We insert both into the merkle store anyway. Now, if the
    /// transaction attempts to verify the merkle path to G, it can do so because F and B' are in
    /// the merkle store. Note that B' is in the store because the transaction inserted it into the
    /// merkle store as part of updating E, not because we inserted it. B is present in the store,
    /// but is simply ignored for the purpose of verifying G's inclusion.
    ///
    /// ## Foreign Accounts
    ///
    /// Foreign accounts are read-only and so their _current_ vault root is always equivalent to the
    /// _initial_ vault root. So, for foreign accounts, just like for the native account, we also
    /// always request witnesses for the initial vault root.
    async fn on_account_vault_asset_witness_requested(
        &self,
        current_account_id: AccountId,
        current_vault_root: Word,
        asset: Asset,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        // For the native account we need to explicitly request the initial vault root, while for
        // foreign accounts the current vault root is always the initial one.
        let vault_root = if current_account_id == self.base_host.initial_account_header().id() {
            self.base_host.initial_account_header().vault_root()
        } else {
            current_vault_root
        };

        let vault_key = asset.vault_key();
        let asset_witness = self
            .base_host
            .store()
            .get_vault_asset_witness(current_account_id, vault_root, vault_key)
            .await
            .map_err(|err| TransactionKernelError::GetVaultAssetWitness {
                vault_root,
                vault_key,
                source: err,
            })?;

        Ok(asset_witness_to_advice_mutation(asset_witness))
    }

    /// Consumes `self` and returns the account delta, output notes, generated signatures and
    /// transaction progress.
    #[allow(clippy::type_complexity)]
    pub fn into_parts(
        self,
    ) -> (
        AccountDelta,
        InputNotes<InputNote>,
        Vec<OutputNote>,
        Vec<AccountCode>,
        BTreeMap<Word, Vec<Felt>>,
        TransactionProgress,
    ) {
        let (account_delta, input_notes, output_notes, tx_progress) = self.base_host.into_parts();

        (
            account_delta,
            input_notes,
            output_notes,
            self.accessed_foreign_account_code,
            self.generated_signatures,
            tx_progress,
        )
    }
}

// HOST IMPLEMENTATION
// ================================================================================================

impl<STORE, AUTH> BaseHost for TransactionExecutorHost<'_, '_, STORE, AUTH>
where
    STORE: DataStore,
    AUTH: TransactionAuthenticator,
{
    fn get_mast_forest(&self, procedure_root: &Word) -> Option<Arc<MastForest>> {
        self.base_host.get_mast_forest(procedure_root)
    }

    fn get_label_and_source_file(
        &self,
        location: &Location,
    ) -> (SourceSpan, Option<Arc<SourceFile>>) {
        let source_manager = self.source_manager.as_ref();
        let maybe_file = source_manager.get_by_uri(location.uri());
        let span = source_manager.location_to_span(location.clone()).unwrap_or_default();
        (span, maybe_file)
    }
}

impl<STORE, AUTH> AsyncHost for TransactionExecutorHost<'_, '_, STORE, AUTH>
where
    STORE: DataStore + Sync,
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
                TransactionEventData::TransactionFeeComputed { fee_asset } => self
                    .on_before_tx_fee_removed_from_account(fee_asset)
                    .await
                    .map_err(EventError::from),
                TransactionEventData::ForeignAccount { account_id } => {
                    self.on_foreign_account_requested(account_id).await.map_err(EventError::from)
                },
                TransactionEventData::AccountVaultAssetWitness {
                    current_account_id,
                    current_vault_root,
                    asset,
                } => self
                    .on_account_vault_asset_witness_requested(
                        current_account_id,
                        current_vault_root,
                        asset,
                    )
                    .await
                    .map_err(EventError::from),
                TransactionEventData::AccountStorageMapWitness {
                    current_account_id,
                    slot_index,
                    current_map_root,
                    map_key,
                } => self
                    .on_account_storage_map_witness_requested(
                        current_account_id,
                        slot_index,
                        current_map_root,
                        map_key,
                    )
                    .await
                    .map_err(EventError::from),
            }
        }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Converts an [`AssetWitness`] into the set of advice mutations that need to be inserted in order
/// to access the asset.
fn asset_witness_to_advice_mutation(asset_witness: AssetWitness) -> Vec<AdviceMutation> {
    // Get the nodes in the proof and insert them into the merkle store.
    let merkle_store_ext = AdviceMutation::extend_merkle_store(asset_witness.authenticated_nodes());

    let smt_proof = SmtProof::from(asset_witness);
    let map_ext = AdviceMutation::extend_map(AdviceMap::from_iter([(
        smt_proof.leaf().hash(),
        smt_proof.leaf().to_elements(),
    )]));

    vec![merkle_store_ext, map_ext]
}

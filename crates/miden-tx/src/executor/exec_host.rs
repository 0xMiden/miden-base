use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::errors::TransactionKernelError;
use miden_lib::transaction::TransactionEvent;
use miden_objects::account::{AccountDelta, PartialAccount};
use miden_objects::assembly::debuginfo::Location;
use miden_objects::assembly::{DefaultSourceManager, SourceFile, SourceManager, SourceSpan};
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
            generated_signatures: BTreeMap::new(),
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
            }
        }
    }
}

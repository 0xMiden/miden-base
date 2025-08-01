use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};

use miden_lib::{errors::TransactionKernelError, transaction::TransactionEvent};
use miden_objects::{
    Felt, Hasher, Word,
    account::{AccountDelta, PartialAccount},
    transaction::{InputNote, InputNotes, OutputNote},
};
use vm_processor::{
    BaseHost, ErrorContext, ExecutionError, MastForest, MastForestStore, ProcessState, SyncHost,
};

use crate::{
    AccountProcedureIndexMap,
    auth::{SigningInputs, TransactionAuthenticator},
    host::{ScriptMastForestStore, TransactionBaseHost, TransactionProgress},
};

/// The transaction executor host is responsible for handling [`SyncHost`] requests made by the
/// transaction kernel during execution. In particular, it responds to signature generation requests
/// by forwarding the request to the contained [`TransactionAuthenticator`].
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
    STORE: MastForestStore,
    AUTH: TransactionAuthenticator,
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
    /// Expected stack state: `[MESSAGE, PUB_KEY]`
    ///
    /// The signature is fetched from the advice map using `hash(PUB_KEY, MESSAGE)` as the key. If
    /// not present in the advice map, the signature is requested from the host's authenticator.
    pub fn on_auth_requested(
        &mut self,
        process: &mut ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let msg = process.get_stack_word(0);
        let pub_key = process.get_stack_word(1);

        let signature_key = Hasher::merge(&[pub_key, msg]);

        let signature = if let Ok(signature) =
            process.advice_provider().get_mapped_values(&signature_key)
        {
            signature.to_vec()
        } else {
            let tx_summary = self.base_host.build_tx_summary(process, msg)?;

            if msg != tx_summary.to_commitment() {
                return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                    "transaction summary doesn't commit to the expected message".into(),
                ));
            }

            let authenticator =
                self.authenticator.ok_or(TransactionKernelError::MissingAuthenticator)?;

            let signing_inputs = SigningInputs::TransactionSummary(Box::new(tx_summary));

            let signature: Vec<Felt> = authenticator
                .get_signature(pub_key, &signing_inputs)
                .map_err(|err| TransactionKernelError::SignatureGenerationFailed(Box::new(err)))?;

            self.generated_signatures.insert(signature_key, signature.clone());

            signature
        };

        process.advice_provider_mut().stack.extend(signature);

        Ok(())
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
}

impl<STORE, AUTH> SyncHost for TransactionExecutorHost<'_, '_, STORE, AUTH>
where
    STORE: MastForestStore,
    AUTH: TransactionAuthenticator,
{
    fn get_mast_forest(&self, procedure_root: &Word) -> Option<Arc<MastForest>> {
        self.base_host.get_mast_forest(procedure_root)
    }

    fn on_event(
        &mut self,
        process: &mut ProcessState<'_>,
        event_id: u32,
        err_ctx: &impl ErrorContext,
    ) -> Result<(), ExecutionError> {
        let transaction_event = TransactionEvent::try_from(event_id)
            .map_err(|err| ExecutionError::event_error(Box::new(err), err_ctx))?;

        match transaction_event {
            // Override the base host's on signature requested implementation, which would not call
            // the authenticator.
            TransactionEvent::AuthRequest => {
                self.on_auth_requested(process)
                    .map_err(|err| ExecutionError::event_error(Box::new(err), err_ctx))?;
            },
            // All other events are handled as in the base host.
            _ => {
                self.base_host.handle_event(process, transaction_event, err_ctx)?;
            },
        }

        Ok(())
    }
}

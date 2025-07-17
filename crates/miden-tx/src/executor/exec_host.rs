use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    vec::Vec,
};

use miden_lib::{errors::TransactionKernelError, transaction::TransactionEvent};
use miden_objects::{
    Felt, Hasher, Word,
    account::{AccountDelta, PartialAccount},
    transaction::OutputNote,
};
use vm_processor::{
    AdviceInputs, BaseHost, ErrorContext, ExecutionError, MastForest, MastForestStore,
    ProcessState, SyncHost,
};

use crate::{
    auth::TransactionAuthenticator,
    errors::TransactionHostError,
    host::{ScriptMastForestStore, TransactionBaseHost, TransactionProgress},
};

/// The transaction executor host is responsible for handling [`SyncHost`] requests made by the
/// transaction kernel during execution. In particular, it responds to signature generation requests
/// by forwarding the request to the contained [`TransactionAuthenticator`].
///
/// Transaction hosts are created on a per-transaction basis. That is, a transaction host is meant
/// to support execution of a single transaction and is discarded after the transaction finishes
/// execution.
pub struct TransactionExecutorHost<'store, 'auth> {
    /// The underlying base transaction host.
    base_host: TransactionBaseHost<'store>,

    /// Serves signature generation requests from the transaction runtime for signatures which are
    /// not present in the `generated_signatures` field.
    authenticator: Option<&'auth dyn TransactionAuthenticator>,

    /// Contains generated signatures (as a message |-> signature map) required for transaction
    /// execution. Once a signature was created for a given message, it is inserted into this map.
    /// After transaction execution, these can be inserted into the advice inputs to re-execute the
    /// transaction without having to regenerate the signature or requiring access to the
    /// authenticator that produced it.
    generated_signatures: BTreeMap<Word, Vec<Felt>>,
}

impl<'store, 'auth> TransactionExecutorHost<'store, 'auth> {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`TransactionExecutorHost`] instance from the provided inputs.
    pub fn new(
        account: &PartialAccount,
        advice_inputs: &mut AdviceInputs,
        mast_store: &'store dyn MastForestStore,
        scripts_mast_store: ScriptMastForestStore,
        authenticator: Option<&'auth dyn TransactionAuthenticator>,
        foreign_account_code_commitments: BTreeSet<Word>,
    ) -> Result<Self, TransactionHostError> {
        let base_host = TransactionBaseHost::new(
            account,
            advice_inputs,
            mast_store,
            scripts_mast_store,
            foreign_account_code_commitments,
        )?;

        Ok(Self {
            base_host,
            authenticator,
            generated_signatures: BTreeMap::new(),
        })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the `tx_progress` field of this transaction host.
    pub(super) fn base_host(&self) -> &TransactionBaseHost {
        &self.base_host
    }

    /// Returns a reference to the `tx_progress` field of this transaction host.
    pub fn tx_progress(&self) -> &TransactionProgress {
        self.base_host.tx_progress()
    }

    // ADVICE INJECTOR HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Pushes a signature to the advice stack as a response to the `FalconSigToStack` injector.
    ///
    /// The signature is fetched from the advice map or otherwise requested from the host's
    /// authenticator.
    pub fn on_signature_requested(
        &mut self,
        process: &mut ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let pub_key = process.get_stack_word(0);
        let msg = process.get_stack_word(1);
        let signature_key = Hasher::merge(&[pub_key, msg]);

        let signature = if let Ok(signature) =
            process.advice_provider().get_mapped_values(&signature_key)
        {
            signature.to_vec()
        } else {
            let account_delta = self.base_host.account_delta_tracker().clone().into_delta();

            let authenticator =
                self.authenticator.ok_or(TransactionKernelError::MissingAuthenticator)?;

            let signature: Vec<Felt> = authenticator
                .get_signature(pub_key, msg, &account_delta)
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

impl BaseHost for TransactionExecutorHost<'_, '_> {}

impl SyncHost for TransactionExecutorHost<'_, '_> {
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
            TransactionEvent::FalconSigToStack => {
                self.on_signature_requested(process)
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

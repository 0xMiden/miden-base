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
    AdviceInputs, AdviceMutation, AsyncHost, BaseHost, EventError, MastForest, MastForestStore,
    ProcessState,
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
        advice_inputs: &mut AdviceInputs,
        mast_store: &'store STORE,
        scripts_mast_store: ScriptMastForestStore,
        authenticator: Option<&'auth AUTH>,
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

    /// Returns a reference to the underlying [`TransactionBaseHost`].
    pub(super) fn base_host(&self) -> &TransactionBaseHost<'store, STORE> {
        &self.base_host
    }

    /// Returns a reference to the `tx_progress` field of this transaction host.
    pub fn tx_progress(&self) -> &TransactionProgress {
        self.base_host.tx_progress()
    }

    // ADVICE INJECTOR HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Pushes a signature to the advice stack as a response to the `AuthRequest` event.
    ///
    /// The signature is fetched from the advice map or otherwise requested from the host's
    /// authenticator.
    pub fn on_signature_requested(
        &mut self,
        process: &ProcessState,
    ) -> impl Future<Output = Result<Vec<AdviceMutation>, TransactionKernelError>> + Send {
        let pub_key = process.get_stack_word(0);
        let msg = process.get_stack_word(1);
        let signature_key = Hasher::merge(&[pub_key, msg]);

        let signature_opt = process
            .advice_provider()
            .get_mapped_values(&signature_key)
            .map(|slice| slice.to_vec());

        async move {
            let signature = if let Some(signature) = signature_opt {
                signature
            } else {
                let account_delta = self.base_host.account_delta_tracker().clone().into_delta();

                let authenticator =
                    self.authenticator.ok_or(TransactionKernelError::MissingAuthenticator)?;

                let signature: Vec<Felt> =
                    authenticator.get_signature(pub_key, msg, &account_delta).await.map_err(
                        |err| TransactionKernelError::SignatureGenerationFailed(Box::new(err)),
                    )?;

                self.generated_signatures.insert(signature_key, signature.clone());

                signature
            };

            Ok(vec![AdviceMutation::ExtendStack { iter: signature }])
        }
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

impl<STORE, AUTH> AsyncHost for TransactionExecutorHost<'_, '_, STORE, AUTH>
where
    STORE: MastForestStore + Sync,
    AUTH: TransactionAuthenticator + Sync,
{
    fn get_mast_forest(
        &self,
        procedure_root: &Word,
    ) -> impl Future<Output = Option<Arc<MastForest>>> + Send {
        core::future::ready(self.base_host.get_mast_forest(procedure_root))
    }

    fn on_event(
        &mut self,
        process: &ProcessState,
        event_id: u32,
    ) -> impl Future<Output = Result<Vec<AdviceMutation>, EventError>> + Send {
        let transaction_event = TransactionEvent::create(process, event_id);

        async move {
            let transaction_event = transaction_event?;
            let advice_mutations = match transaction_event {
                // Override the base host's on signature requested implementation, which would not
                // call the authenticator.
                TransactionEvent::AuthRequest => {
                    // self.on_signature_requested(process).await.map_err(Box::new)?
                    vec![]
                },
                // All other events are handled as in the base host.
                _ => {
                    // self.base_host.handle_event(process, transaction_event)?
                    vec![]
                },
            };

            Ok(advice_mutations)
        }
    }
}

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
    executor::build_tx_summary,
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
            // Retrieve transaction summary commitments from the advice provider.
            // The commitments are stored as a contiguous array of field elements with the following
            // layout:
            // - commitments[0..4]:  SALT
            // - commitments[4..8]:  OUTPUT_NOTES_COMMITMENT
            // - commitments[8..12]: INPUT_NOTES_COMMITMENT
            // - commitments[12..16]: ACCOUNT_DELTA_COMMITMENT
            let commitments = process.advice_provider().get_mapped_values(&msg).map_err(|err| {
                TransactionKernelError::TransactionSummaryConstructionFailed(Box::new(err))
            })?;

            if commitments.len() != 16 {
                return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                    "expected 4 words for transaction summary commitments".into(),
                ));
            }

            let salt = extract_word(&commitments, 0);
            let output_notes_commitment = extract_word(&commitments, 4);
            let input_notes_commitment = extract_word(&commitments, 8);
            let account_delta_commitment = extract_word(&commitments, 12);
            let tx_summary = build_tx_summary(
                self.base_host(),
                salt,
                output_notes_commitment,
                input_notes_commitment,
                account_delta_commitment,
            )
            .map_err(|err| {
                TransactionKernelError::TransactionSummaryConstructionFailed(Box::new(err))
            })?;

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

// HELPER FUNCTIONS
// ================================================================================================

/// Extracts a word from a slice of field elements.
fn extract_word(commitments: &[Felt], start: usize) -> Word {
    Word::from([
        commitments[start],
        commitments[start + 1],
        commitments[start + 2],
        commitments[start + 3],
    ])
}

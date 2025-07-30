use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};

use miden_lib::{
    errors::TransactionKernelError,
    transaction::{TransactionEvent, TransactionEventData, TransactionEventHandling},
};
use miden_objects::{
    Felt, Word,
    account::{AccountDelta, PartialAccount},
    assembly::{
        DefaultSourceManager, SourceManager,
        debuginfo::{Location, SourceFile, SourceSpan},
    },
    transaction::{InputNote, InputNotes, OutputNote},
};
use vm_processor::{
    AdviceMutation, AsyncHost, AsyncHostFuture, BaseHost, EventError, MastForest, MastForestStore,
    ProcessState,
};

use crate::{
    AccountProcedureIndexMap,
    auth::{SigningInputs, TransactionAuthenticator},
    executor::build_tx_summary,
    host::{ScriptMastForestStore, TransactionBaseHost, TransactionProgress},
};

/// The transaction executor host is responsible for handling [`AsyncHost`] requests made by the
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
    async fn on_signature_requested(
        &mut self,
        pub_key_hash: Word,
        message: Word,
        signature_key: Word,
        signature_opt: Option<Vec<Felt>>,
        commitments_opt: Option<Vec<Felt>>,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        let signature = if let Some(signature) = signature_opt {
            signature.to_vec()
        } else {
            // Retrieve transaction summary commitments from the advice provider.
            // The commitments are stored as a contiguous array of field elements with the following
            // layout:
            // - commitments[0..4]:  SALT
            // - commitments[4..8]:  OUTPUT_NOTES_COMMITMENT
            // - commitments[8..12]: INPUT_NOTES_COMMITMENT
            // - commitments[12..16]: ACCOUNT_DELTA_COMMITMENT
            let commitments = commitments_opt.ok_or_else(|| {
                TransactionKernelError::TransactionSummaryConstructionFailed(Box::from(
                    "expected commitments to be present in advice provider",
                ))
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

            if message != tx_summary.to_commitment() {
                return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                    "transaction summary doesn't commit to the expected message".into(),
                ));
            }

            let authenticator =
                self.authenticator.ok_or(TransactionKernelError::MissingAuthenticator)?;

            let signing_inputs = SigningInputs::TransactionSummary(Box::new(tx_summary));

            let signature: Vec<Felt> = authenticator
                .get_signature(pub_key_hash, &signing_inputs)
                .await
                .map_err(|err| TransactionKernelError::SignatureGenerationFailed(Box::new(err)))?;

            self.generated_signatures.insert(signature_key, signature.clone());

            signature
        };

        Ok(vec![AdviceMutation::ExtendStack { values: signature }])
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
    fn get_mast_forest(&self, procedure_root: &Word) -> Option<Arc<MastForest>> {
        self.base_host.get_mast_forest(procedure_root)
    }

    fn on_event(
        &mut self,
        process: &ProcessState,
        event_id: u32,
    ) -> impl AsyncHostFuture<Result<Vec<AdviceMutation>, EventError>> {
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
                TransactionEventData::AuthRequest {
                    pub_key_hash,
                    message,
                    signature_key,
                    signature_opt,
                    commitments_opt,
                } => self
                    .on_signature_requested(
                        pub_key_hash,
                        message,
                        signature_key,
                        signature_opt,
                        commitments_opt,
                    )
                    .await
                    .map_err(EventError::from),
            }
        }
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

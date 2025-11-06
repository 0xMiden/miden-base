use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_objects::Word;
use miden_objects::account::{AccountDelta, PartialAccount};
use miden_objects::assembly::debuginfo::Location;
use miden_objects::assembly::{SourceFile, SourceSpan};
use miden_objects::transaction::{InputNote, InputNotes, OutputNote};
use miden_processor::{
    AdviceMutation,
    BaseHost,
    EventError,
    MastForest,
    MastForestStore,
    ProcessState,
    SyncHost,
};

use crate::host::{
    ScriptMastForestStore,
    TransactionBaseHost,
    TransactionEvent,
    TransactionProgress,
};
use crate::{AccountProcedureIndexMap, TransactionKernelError};

/// The transaction prover host is responsible for handling [`SyncHost`] requests made by the
/// transaction kernel during proving.
pub struct TransactionProverHost<'store, STORE>
where
    STORE: MastForestStore,
{
    /// The underlying base transaction host.
    base_host: TransactionBaseHost<'store, STORE>,
}

impl<'store, STORE> TransactionProverHost<'store, STORE>
where
    STORE: MastForestStore,
{
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`TransactionProverHost`] instance from the provided inputs.
    pub fn new(
        account: &PartialAccount,
        input_notes: InputNotes<InputNote>,
        mast_store: &'store STORE,
        scripts_mast_store: ScriptMastForestStore,
        acct_procedure_index_map: AccountProcedureIndexMap,
    ) -> Self {
        let base_host = TransactionBaseHost::new(
            account,
            input_notes,
            mast_store,
            scripts_mast_store,
            acct_procedure_index_map,
        );

        Self { base_host }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the `tx_progress` field of this transaction host.
    pub fn tx_progress(&self) -> &TransactionProgress {
        self.base_host.tx_progress()
    }

    /// Consumes `self` and returns the account delta, output notes and transaction progress.
    pub fn into_parts(
        self,
    ) -> (AccountDelta, InputNotes<InputNote>, Vec<OutputNote>, TransactionProgress) {
        self.base_host.into_parts()
    }
}

// HOST IMPLEMENTATION
// ================================================================================================

impl<STORE> BaseHost for TransactionProverHost<'_, STORE>
where
    STORE: MastForestStore,
{
    fn get_label_and_source_file(
        &self,
        _location: &Location,
    ) -> (SourceSpan, Option<Arc<SourceFile>>) {
        // For the prover, we assume that the transaction witness is a successfully executed
        // transaction and so there should be no need to provide the actual source manager, as it
        // is only used to improve error message quality which we shouldn't run into here.
        (SourceSpan::UNKNOWN, None)
    }
}

impl<STORE> SyncHost for TransactionProverHost<'_, STORE>
where
    STORE: MastForestStore,
{
    fn get_mast_forest(&self, node_digest: &Word) -> Option<Arc<MastForest>> {
        self.base_host.get_mast_forest(node_digest)
    }

    fn on_event(&mut self, process: &ProcessState) -> Result<Vec<AdviceMutation>, EventError> {
        if let Some(advice_mutations) = self.base_host.handle_stdlib_events(process)? {
            return Ok(advice_mutations);
        }

        let tx_event = TransactionEvent::extract_from_process(process).map_err(EventError::from)?;

        // None means the event ID does not need to be handled.
        let Some(tx_event) = tx_event else {
            return Ok(Vec::new());
        };

        let result = match tx_event {
            // Foreign account data and witnesses should be in the advice provider at
            // proving time, so there is nothing to do.
            TransactionEvent::AccountBeforeForeignLoad { .. } => Ok(Vec::new()),

            TransactionEvent::AccountVaultAfterRemoveAsset { asset } => {
                self.base_host.on_account_vault_after_remove_asset(asset)
            },
            TransactionEvent::AccountVaultAfterAddAsset { asset } => {
                self.base_host.on_account_vault_after_add_asset(asset)
            },

            TransactionEvent::AccountStorageAfterSetItem { slot_idx, current_value, new_value } => {
                self.base_host
                    .on_account_storage_after_set_item(slot_idx, current_value, new_value)
            },

            TransactionEvent::AccountStorageAfterSetMapItem {
                slot_index,
                key,
                prev_map_value,
                new_map_value,
            } => self.base_host.on_account_storage_after_set_map_item(
                slot_index,
                key,
                prev_map_value,
                new_map_value,
            ),

            // Access witnesses should be in the advice provider at proving time.
            TransactionEvent::AccountVaultBeforeAssetAccess { .. } => Ok(Vec::new()),
            TransactionEvent::AccountStorageBeforeMapItemAccess { .. } => Ok(Vec::new()),

            TransactionEvent::AccountAfterIncrementNonce => {
                self.base_host.on_account_after_increment_nonce()
            },

            TransactionEvent::AccountPushProcedureIndex { code_commitment, procedure_root } => {
                self.base_host.on_account_push_procedure_index(code_commitment, procedure_root)
            },

            TransactionEvent::NoteAfterCreated {
                note_idx,
                metadata,
                recipient_digest,
                note_script,
                recipient_data,
            } => {
                let recipient_data = self.base_host.on_note_after_created(
                    note_idx,
                    metadata,
                    recipient_digest,
                    note_script,
                    recipient_data,
                )?;

                // A return value of Some means recipient data was present, but the script was not
                // and this should not happen at proving time.
                if recipient_data.is_some() {
                    Err(TransactionKernelError::other(
                        "note script should be in the advice provider at proving time",
                    ))
                } else {
                    // A return value of None means the note creation was handled.
                    Ok(Vec::new())
                }
            },

            TransactionEvent::NoteBeforeAddAsset { note_idx, asset } => {
                self.base_host.on_note_before_add_asset(note_idx, asset).map(|_| Vec::new())
            },

            // The base host should have handled this event since the signature should be
            // present in the advice map.
            TransactionEvent::AuthRequest { signature, .. } => {
                if let Some(signature) = signature {
                    Ok(self.base_host.on_auth_requested(signature))
                } else {
                    Err(TransactionKernelError::other(
                        "signatures should be in the advice provider at proving time",
                    ))
                }
            },

            // This always returns an error to abort the transaction.
            TransactionEvent::Unauthorized {
                message,
                salt,
                output_notes_commitment,
                input_notes_commitment,
                account_delta_commitment,
            } => Err(self.base_host.on_unauthorized(
                message,
                salt,
                output_notes_commitment,
                input_notes_commitment,
                account_delta_commitment,
            )),

            TransactionEvent::PrologueStart { clk } => {
                self.base_host.tx_progress_mut().start_prologue(clk);
                Ok(Vec::new())
            },
            TransactionEvent::PrologueEnd { clk } => {
                self.base_host.tx_progress_mut().end_prologue(clk);
                Ok(Vec::new())
            },

            TransactionEvent::NotesProcessingStart { clk } => {
                self.base_host.tx_progress_mut().start_notes_processing(clk);
                Ok(Vec::new())
            },
            TransactionEvent::NotesProcessingEnd { clk } => {
                self.base_host.tx_progress_mut().end_notes_processing(clk);
                Ok(Vec::new())
            },

            TransactionEvent::NoteExecutionStart { note_id, clk } => {
                self.base_host.tx_progress_mut().start_note_execution(clk, note_id);
                Ok(Vec::new())
            },
            TransactionEvent::NoteExecutionEnd { clk } => {
                self.base_host.tx_progress_mut().end_note_execution(clk);
                Ok(Vec::new())
            },

            TransactionEvent::TxScriptProcessingStart { clk } => {
                self.base_host.tx_progress_mut().start_tx_script_processing(clk);
                Ok(Vec::new())
            },
            TransactionEvent::TxScriptProcessingEnd { clk } => {
                self.base_host.tx_progress_mut().end_tx_script_processing(clk);
                Ok(Vec::new())
            },

            TransactionEvent::EpilogueStart { clk } => {
                self.base_host.tx_progress_mut().start_epilogue(clk);
                Ok(Vec::new())
            },
            TransactionEvent::EpilogueEnd { clk } => {
                self.base_host.tx_progress_mut().end_epilogue(clk);
                Ok(Vec::new())
            },

            TransactionEvent::EpilogueAuthProcStart { clk } => {
                self.base_host.tx_progress_mut().start_auth_procedure(clk);
                Ok(Vec::new())
            },
            TransactionEvent::EpilogueAuthProcEnd { clk } => {
                self.base_host.tx_progress_mut().end_auth_procedure(clk);
                Ok(Vec::new())
            },

            TransactionEvent::EpilogueAfterTxCyclesObtained { clk } => {
                self.base_host.tx_progress_mut().epilogue_after_tx_cycles_obtained(clk);
                Ok(Vec::new())
            },

            // We don't track enough information to handle this event. Since this just improves
            // error messages for users and the error should not be relevant during proving, we
            // ignore it.
            TransactionEvent::EpilogueBeforeTxFeeRemovedFromAccount { .. } => Ok(Vec::new()),

            TransactionEvent::LinkMapSet { advice_mutation } => Ok(advice_mutation),
            TransactionEvent::LinkMapGet { advice_mutation } => Ok(advice_mutation),
        };

        result.map_err(EventError::from)
    }
}

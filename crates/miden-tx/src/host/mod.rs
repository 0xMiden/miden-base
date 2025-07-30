mod account_delta_tracker;
use account_delta_tracker::AccountDeltaTracker;

mod storage_delta_tracker;

mod link_map;
pub use link_map::LinkMap;

mod account_procedures;
pub use account_procedures::AccountProcedureIndexMap;

mod note_builder;
use note_builder::OutputNoteBuilder;

mod script_mast_forest_store;
pub use script_mast_forest_store::ScriptMastForestStore;

mod tx_progress;
use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};

use miden_lib::transaction::{
    TransactionEvent, TransactionEventError, TransactionKernelError,
    memory::{CURRENT_INPUT_NOTE_PTR, NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR},
};
use miden_objects::{
    Hasher, Word,
    account::{AccountDelta, PartialAccount},
    asset::Asset,
    note::NoteId,
    transaction::{
        InputNote, InputNotes, OutputNote, OutputNotes, TransactionMeasurements, TransactionSummary,
    },
    vm::RowIndex,
};
pub use tx_progress::TransactionProgress;
use vm_processor::{
    ContextId, ErrorContext, ExecutionError, Felt, MastForest, MastForestStore, MemoryError,
    ProcessState,
};

// TRANSACTION BASE HOST
// ================================================================================================

/// The base transaction host that implements shared behavior of all transaction host
/// implementations.
pub struct TransactionBaseHost<'store, STORE> {
    /// MAST store which contains the code required to execute account code functions.
    mast_store: &'store STORE,

    /// MAST store which contains the forests of all scripts involved in the transaction. These
    /// include input note scripts and the transaction script, but not account code.
    scripts_mast_store: ScriptMastForestStore,

    /// Account state changes accumulated during transaction execution.
    ///
    /// The delta is updated by event handlers.
    account_delta: AccountDeltaTracker,

    /// A map of the procedure MAST roots to the corresponding procedure indices for all the
    /// account codes involved in the transaction (for native and foreign accounts alike).
    acct_procedure_index_map: AccountProcedureIndexMap,

    /// Input notes consumed by the transaction.
    input_notes: InputNotes<InputNote>,

    /// The list of notes created while executing a transaction stored as note_ptr |-> note_builder
    /// map.
    output_notes: BTreeMap<usize, OutputNoteBuilder>,

    /// Tracks the number of cycles for each of the transaction execution stages.
    ///
    /// The progress is updated event handlers.
    tx_progress: TransactionProgress,
}

impl<'store, STORE> TransactionBaseHost<'store, STORE>
where
    STORE: MastForestStore,
{
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`TransactionBaseHost`] instance from the provided inputs.
    pub fn new(
        account: &PartialAccount,
        input_notes: InputNotes<InputNote>,
        mast_store: &'store STORE,
        scripts_mast_store: ScriptMastForestStore,
        acct_procedure_index_map: AccountProcedureIndexMap,
    ) -> Self {
        Self {
            mast_store,
            scripts_mast_store,
            account_delta: AccountDeltaTracker::new(
                account.id(),
                account.storage().header().clone(),
            ),
            acct_procedure_index_map,
            output_notes: BTreeMap::default(),
            input_notes,
            tx_progress: TransactionProgress::default(),
        }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`MastForest`] that contains the procedure with the given `procedure_root`.
    pub fn get_mast_forest(&self, procedure_root: &Word) -> Option<Arc<MastForest>> {
        // Search in the note MAST forest store, otherwise fall back to the user-provided store
        match self.scripts_mast_store.get(procedure_root) {
            Some(forest) => Some(forest),
            None => self.mast_store.get(procedure_root),
        }
    }

    /// Returns a reference to the `tx_progress` field of this transaction host.
    pub fn tx_progress(&self) -> &TransactionProgress {
        &self.tx_progress
    }

    /// Returns a reference to the account delta tracker of this transaction host.
    pub fn account_delta_tracker(&self) -> &AccountDeltaTracker {
        &self.account_delta
    }

    /// Clones the inner [`AccountDeltaTracker`] and converts it into an [`AccountDelta`].
    pub fn build_account_delta(&self) -> AccountDelta {
        self.account_delta_tracker().clone().into_delta()
    }

    /// Returns the input notes consumed in this transaction.
    #[allow(unused)]
    pub fn input_notes(&self) -> InputNotes<InputNote> {
        self.input_notes.clone()
    }

    /// Clones the inner [`OutputNoteBuilder`]s and returns the vector of created output notes that
    /// are tracked by this host.
    pub fn build_output_notes(&self) -> Vec<OutputNote> {
        self.output_notes.values().cloned().map(|builder| builder.build()).collect()
    }

    /// Consumes `self` and returns the account delta, output notes and transaction progress.
    pub fn into_parts(self) -> (AccountDelta, Vec<OutputNote>, TransactionProgress) {
        let output_notes = self.output_notes.into_values().map(|builder| builder.build()).collect();

        (self.account_delta.into_delta(), output_notes, self.tx_progress)
    }

    // EVENT HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Handles the given [`TransactionEvent`], for example by updating the account delta or pushing
    /// requested advice to the advice stack.
    pub fn handle_event(
        &mut self,
        process: &mut ProcessState,
        transaction_event: TransactionEvent,
        err_ctx: &impl ErrorContext,
    ) -> Result<(), ExecutionError> {
        // Privileged events can only be emitted from the root context.
        if process.ctx() != ContextId::root() && transaction_event.is_privileged() {
            return Err(ExecutionError::event_error(
                Box::new(TransactionEventError::NotRootContext(transaction_event as u32)),
                err_ctx,
            ));
        }

        match transaction_event {
            TransactionEvent::AccountVaultBeforeAddAsset => Ok(()),
            TransactionEvent::AccountVaultAfterAddAsset => {
                self.on_account_vault_after_add_asset(process)
            },

            TransactionEvent::AccountVaultBeforeRemoveAsset => Ok(()),
            TransactionEvent::AccountVaultAfterRemoveAsset => {
                self.on_account_vault_after_remove_asset(process)
            },

            TransactionEvent::AccountStorageBeforeSetItem => Ok(()),
            TransactionEvent::AccountStorageAfterSetItem => {
                self.on_account_storage_after_set_item(process)
            },

            TransactionEvent::AccountStorageBeforeSetMapItem => Ok(()),
            TransactionEvent::AccountStorageAfterSetMapItem => {
                self.on_account_storage_after_set_map_item(process)
            },

            TransactionEvent::AccountBeforeIncrementNonce => {
                Ok(())
            },
            TransactionEvent::AccountAfterIncrementNonce => {
                self.on_account_after_increment_nonce()
            },

            TransactionEvent::AccountPushProcedureIndex => {
                self.on_account_push_procedure_index(process)
            },

            TransactionEvent::NoteBeforeCreated => Ok(()),
            TransactionEvent::NoteAfterCreated => self.on_note_after_created(process),

            TransactionEvent::NoteBeforeAddAsset => self.on_note_before_add_asset(process),
            TransactionEvent::NoteAfterAddAsset => Ok(()),

            TransactionEvent::AuthRequest => self.on_signature_requested(process),

            TransactionEvent::PrologueStart => {
                self.tx_progress.start_prologue(process.clk());
                Ok(())
            },
            TransactionEvent::PrologueEnd => {
                self.tx_progress.end_prologue(process.clk());
                Ok(())
            },

            TransactionEvent::NotesProcessingStart => {
                self.tx_progress.start_notes_processing(process.clk());
                Ok(())
            },
            TransactionEvent::NotesProcessingEnd => {
                self.tx_progress.end_notes_processing(process.clk());
                Ok(())
            },

            TransactionEvent::NoteExecutionStart => {
                let note_id = Self::get_current_note_id(process,err_ctx)?.expect(
                    "Note execution interval measurement is incorrect: check the placement of the start and the end of the interval",
                );
                self.tx_progress.start_note_execution(process.clk(), note_id);
                Ok(())
            },
            TransactionEvent::NoteExecutionEnd => {
                self.tx_progress.end_note_execution(process.clk());
                Ok(())
            },

            TransactionEvent::TxScriptProcessingStart => {
                self.tx_progress.start_tx_script_processing(process.clk());
                Ok(())
            }
            TransactionEvent::TxScriptProcessingEnd => {
                self.tx_progress.end_tx_script_processing(process.clk());
                Ok(())
            }

            TransactionEvent::EpilogueStart => {
                self.tx_progress.start_epilogue(process.clk());
                Ok(())
            }
            TransactionEvent::EpilogueEnd => {
                self.tx_progress.end_epilogue(process.clk());
                Ok(())
            }
            TransactionEvent::LinkMapSetEvent => {
                LinkMap::handle_set_event(process)?;
                Ok(())
            },
            TransactionEvent::LinkMapGetEvent => {
                LinkMap::handle_get_event(process)?;
                Ok(())
            },
            TransactionEvent::Unauthorized => {
              // Note: This always returns an error to abort the transaction.
              Err(self.on_unauthorized(process))
            }
        }
        .map_err(|err| ExecutionError::event_error(Box::new(err),err_ctx))?;

        Ok(())
    }

    /// Pushes a signature to the advice stack as a response to the `AuthRequest` event.
    ///
    /// The signature is fetched from the advice map and if it is not present, an error is returned.
    pub fn on_signature_requested(
        &mut self,
        process: &mut ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let pub_key = process.get_stack_word(0);
        let msg = process.get_stack_word(1);

        let signature_key = Hasher::merge(&[pub_key, msg]);

        let signature = process
            .advice_provider()
            .get_mapped_values(&signature_key)
            .map_err(|_| TransactionKernelError::MissingAuthenticator)?
            .to_vec();

        process.advice_provider_mut().stack.extend(signature);

        Ok(())
    }

    /// Creates a new [OutputNoteBuilder] from the data on the operand stack and stores it into the
    /// `output_notes` field of this [`TransactionBaseHost`].
    ///
    /// Expected stack state: `[NOTE_METADATA, RECIPIENT, ...]`
    fn on_note_after_created(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let stack = process.get_stack_state();
        // # => [NOTE_METADATA]

        let note_idx: usize = stack[9].as_int() as usize;

        assert_eq!(note_idx, self.output_notes.len(), "note index mismatch");

        let note_builder = OutputNoteBuilder::new(stack, process.advice_provider())?;

        self.output_notes.insert(note_idx, note_builder);

        Ok(())
    }

    /// Adds an asset at the top of the [OutputNoteBuilder] identified by the note pointer.
    ///
    /// Expected stack state: [ASSET, note_ptr, num_of_assets, note_idx]
    fn on_note_before_add_asset(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let stack = process.get_stack_state();
        //# => [ASSET, note_ptr, num_of_assets, note_idx]

        let note_idx = stack[6].as_int();
        assert!(note_idx < self.output_notes.len() as u64);
        let node_idx = note_idx as usize;

        let asset = Asset::try_from(process.get_stack_word(0)).map_err(|source| {
            TransactionKernelError::MalformedAssetInEventHandler {
                handler: "on_note_before_add_asset",
                source,
            }
        })?;

        let note_builder = self
            .output_notes
            .get_mut(&node_idx)
            .ok_or_else(|| TransactionKernelError::MissingNote(note_idx))?;

        note_builder.add_asset(asset)?;

        Ok(())
    }

    /// Loads the index of the procedure root onto the advice stack.
    ///
    /// Expected stack state: [PROC_ROOT, ...]
    fn on_account_push_procedure_index(
        &mut self,
        process: &mut ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let proc_idx = self.acct_procedure_index_map.get_proc_index(process)?;
        process.advice_provider_mut().push_stack(Felt::from(proc_idx));
        Ok(())
    }

    /// Handles the increment nonce event by incrementing the nonce delta by one.
    pub fn on_account_after_increment_nonce(&mut self) -> Result<(), TransactionKernelError> {
        if self.account_delta.was_nonce_incremented() {
            return Err(TransactionKernelError::NonceCanOnlyIncrementOnce);
        }

        self.account_delta.increment_nonce();
        Ok(())
    }

    // ACCOUNT STORAGE UPDATE HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Extracts information from the process state about the storage slot being updated and
    /// records the latest value of this storage slot.
    ///
    /// Expected stack state: [slot_index, NEW_SLOT_VALUE, CURRENT_SLOT_VALUE, ...]
    pub fn on_account_storage_after_set_item(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        // get slot index from the stack and make sure it is valid
        let slot_index = process.get_stack_item(0);

        // get number of storage slots initialized by the account
        let num_storage_slot = Self::get_num_storage_slots(process)?;

        if slot_index.as_int() >= num_storage_slot {
            return Err(TransactionKernelError::InvalidStorageSlotIndex {
                max: num_storage_slot,
                actual: slot_index.as_int(),
            });
        }

        // get the value to which the slot is being updated
        let new_slot_value = Word::new([
            process.get_stack_item(4),
            process.get_stack_item(3),
            process.get_stack_item(2),
            process.get_stack_item(1),
        ]);

        // get the current value for the slot
        let current_slot_value = Word::new([
            process.get_stack_item(8),
            process.get_stack_item(7),
            process.get_stack_item(6),
            process.get_stack_item(5),
        ]);

        self.account_delta.storage().set_item(
            slot_index.as_int() as u8,
            current_slot_value,
            new_slot_value,
        );

        Ok(())
    }

    /// Extracts information from the process state about the storage map being updated and
    /// records the latest values of this storage map.
    ///
    /// Expected stack state: [slot_index, KEY, PREV_MAP_VALUE, NEW_MAP_VALUE]
    pub fn on_account_storage_after_set_map_item(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        // get slot index from the stack and make sure it is valid
        let slot_index = process.get_stack_item(0);

        // get number of storage slots initialized by the account
        let num_storage_slot = Self::get_num_storage_slots(process)?;

        if slot_index.as_int() >= num_storage_slot {
            return Err(TransactionKernelError::InvalidStorageSlotIndex {
                max: num_storage_slot,
                actual: slot_index.as_int(),
            });
        }

        // get the KEY to which the slot is being updated
        let key = Word::new([
            process.get_stack_item(4),
            process.get_stack_item(3),
            process.get_stack_item(2),
            process.get_stack_item(1),
        ]);

        // get the previous VALUE of the slot
        let prev_map_value = Word::new([
            process.get_stack_item(8),
            process.get_stack_item(7),
            process.get_stack_item(6),
            process.get_stack_item(5),
        ]);

        // get the VALUE to which the slot is being updated
        let new_map_value = Word::new([
            process.get_stack_item(12),
            process.get_stack_item(11),
            process.get_stack_item(10),
            process.get_stack_item(9),
        ]);

        self.account_delta.storage().set_map_item(
            slot_index.as_int() as u8,
            key,
            prev_map_value,
            new_map_value,
        );

        Ok(())
    }

    // ACCOUNT VAULT UPDATE HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Extracts the asset that is being added to the account's vault from the process state and
    /// updates the appropriate fungible or non-fungible asset map.
    ///
    /// Expected stack state: [ASSET, ...]
    pub fn on_account_vault_after_add_asset(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let asset: Asset = process.get_stack_word(0).try_into().map_err(|source| {
            TransactionKernelError::MalformedAssetInEventHandler {
                handler: "on_account_vault_after_add_asset",
                source,
            }
        })?;

        self.account_delta
            .vault_delta()
            .add_asset(asset)
            .map_err(TransactionKernelError::AccountDeltaAddAssetFailed)?;
        Ok(())
    }

    /// Extracts the asset that is being removed from the account's vault from the process state
    /// and updates the appropriate fungible or non-fungible asset map.
    ///
    /// Expected stack state: [ASSET, ...]
    pub fn on_account_vault_after_remove_asset(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let asset: Asset = process.get_stack_word(0).try_into().map_err(|source| {
            TransactionKernelError::MalformedAssetInEventHandler {
                handler: "on_account_vault_after_remove_asset",
                source,
            }
        })?;

        self.account_delta
            .vault_delta()
            .remove_asset(asset)
            .map_err(TransactionKernelError::AccountDeltaRemoveAssetFailed)?;
        Ok(())
    }

    // TRANSACTION SUMMARY HELPER
    // --------------------------------------------------------------------------------------------

    /// Builds a [TransactionSummary] by extracting data from the advice provider and validating
    /// commitments against the host's state.
    pub(crate) fn build_tx_summary(
        &self,
        process: &ProcessState,
        msg: Word,
    ) -> Result<TransactionSummary, TransactionKernelError> {
        let commitments = process.advice_provider().get_mapped_values(&msg).map_err(|err| {
            TransactionKernelError::TransactionSummaryConstructionFailed(Box::new(err))
        })?;

        if commitments.len() != 16 {
            return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                "Expected 4 words for transaction summary commitments".into(),
            ));
        }

        let salt = extract_word(commitments, 0);
        let output_notes_commitment = extract_word(commitments, 4);
        let input_notes_commitment = extract_word(commitments, 8);
        let account_delta_commitment = extract_word(commitments, 12);

        let account_delta = self.build_account_delta();
        let input_notes = self.input_notes();
        let output_notes_vec = self.build_output_notes();
        let output_notes = OutputNotes::new(output_notes_vec).map_err(|err| {
            TransactionKernelError::TransactionSummaryConstructionFailed(Box::new(err))
        })?;

        // Validate commitments
        let actual_account_delta_commitment = account_delta.to_commitment();
        if actual_account_delta_commitment != account_delta_commitment {
            return Err(TransactionKernelError::TransactionSummaryCommitmentMismatch(
                format!(
                    "expected account delta commitment to be {actual_account_delta_commitment} but was {account_delta_commitment}"
                )
                .into(),
            ));
        }

        let actual_input_notes_commitment = input_notes.commitment();
        if actual_input_notes_commitment != input_notes_commitment {
            return Err(TransactionKernelError::TransactionSummaryCommitmentMismatch(
                format!(
                    "expected input notes commitment to be {actual_input_notes_commitment} but was {input_notes_commitment}"
                )
                .into(),
            ));
        }

        let actual_output_notes_commitment = output_notes.commitment();
        if actual_output_notes_commitment != output_notes_commitment {
            return Err(TransactionKernelError::TransactionSummaryCommitmentMismatch(
                format!(
                    "expected output notes commitment to be {actual_output_notes_commitment} but was {output_notes_commitment}"
                )
                .into(),
            ));
        }

        Ok(TransactionSummary::new(account_delta, input_notes, output_notes, salt))
    }

    /// Aborts the transaction by building the
    /// [`TransactionSummary`](miden_objects::transaction::TransactionSummary) based on elements on
    /// the operand stack and advice map.
    ///
    /// Expected stack state:
    ///
    /// ```text
    /// [<word>, MESSAGE]
    /// ```
    ///
    /// Expected advice map state:
    ///
    /// ```text
    /// MESSAGE -> [SALT, OUTPUT_NOTES_COMMITMENT, INPUT_NOTES_COMMITMENT, ACCOUNT_DELTA_COMMITMENT]
    /// ```
    fn on_unauthorized(&self, process: &mut ProcessState) -> TransactionKernelError {
        let msg = process.get_stack_word(1);

        let tx_summary = match self.build_tx_summary(process, msg) {
            Ok(s) => s,
            Err(err) => return err,
        };

        if msg != tx_summary.to_commitment() {
            return TransactionKernelError::TransactionSummaryConstructionFailed(
                "transaction summary doesn't commit to the expected message".into(),
            );
        }

        TransactionKernelError::Unauthorized(Box::new(tx_summary))
    }

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Returns the ID of the currently executing input note, or None if the note execution hasn't
    /// started yet or has already ended.
    ///
    /// # Errors
    /// Returns an error if the address of the currently executing input note is invalid (e.g.,
    /// greater than `u32::MAX`).
    fn get_current_note_id(
        process: &ProcessState,
        err_ctx: &impl ErrorContext,
    ) -> Result<Option<NoteId>, ExecutionError> {
        // get the note address in `Felt` or return `None` if the address hasn't been accessed
        // previously.
        let note_address_felt = match process.get_mem_value(process.ctx(), CURRENT_INPUT_NOTE_PTR) {
            Some(addr) => addr,
            None => return Ok(None),
        };
        // convert note address into u32
        let note_address: u32 = note_address_felt.try_into().map_err(|_| {
            ExecutionError::MemoryError(MemoryError::address_out_of_bounds(
                note_address_felt.as_int(),
                err_ctx,
            ))
        })?;
        // if `note_address` == 0 note execution has ended and there is no valid note address
        if note_address == 0 {
            Ok(None)
        } else {
            Ok(process
                .get_mem_word(process.ctx(), note_address)
                .map_err(ExecutionError::MemoryError)?
                .map(NoteId::from))
        }
    }

    /// Returns the number of storage slots initialized for the current account.
    ///
    /// # Errors
    /// Returns an error if the memory location supposed to contain the account storage slot number
    /// has not been initialized.
    fn get_num_storage_slots(process: &ProcessState) -> Result<u64, TransactionKernelError> {
        let num_storage_slots_felt = process
            .get_mem_value(process.ctx(), NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR)
            .ok_or(TransactionKernelError::AccountStorageSlotsNumMissing(
                NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR,
            ))?;

        Ok(num_storage_slots_felt.as_int())
    }
}

/// Extracts a word from a slice of field elements.
pub(crate) fn extract_word(commitments: &[Felt], start: usize) -> Word {
    Word::from([
        commitments[start],
        commitments[start + 1],
        commitments[start + 2],
        commitments[start + 3],
    ])
}

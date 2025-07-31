mod account_delta_tracker;
use core::error::Error;

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
    TransactionEvent, TransactionEventData, TransactionEventError, TransactionEventHandling,
    TransactionKernelError,
    memory::{CURRENT_INPUT_NOTE_PTR, NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR},
};
use miden_objects::{
    Hasher, Word,
    account::{AccountDelta, PartialAccount},
    asset::Asset,
    note::NoteId,
    transaction::{InputNote, InputNotes, OutputNote, TransactionMeasurements},
    vm::RowIndex,
};
pub use tx_progress::TransactionProgress;
use vm_processor::{
    AdviceMutation, ContextId, EventError, ExecutionError, Felt, MastForest, MastForestStore,
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
        process: &ProcessState,
        transaction_event: TransactionEvent,
    ) -> Result<TransactionEventHandling, EventError> {
        // Privileged events can only be emitted from the root context.
        if process.ctx() != ContextId::root() && transaction_event.is_privileged() {
            return Err(Box::new(TransactionEventError::NotRootContext(transaction_event as u32)));
        }

        let advice_mutations = match transaction_event {
            TransactionEvent::AccountVaultBeforeAddAsset => Ok(TransactionEventHandling::Handled(Vec::new())),
            TransactionEvent::AccountVaultAfterAddAsset => {
                self.on_account_vault_after_add_asset(process).map(|_| TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::AccountVaultBeforeRemoveAsset => Ok(TransactionEventHandling::Handled(Vec::new())),
            TransactionEvent::AccountVaultAfterRemoveAsset => {
                self.on_account_vault_after_remove_asset(process).map(|_| TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::AccountStorageBeforeSetItem => Ok(TransactionEventHandling::Handled(Vec::new())),
            TransactionEvent::AccountStorageAfterSetItem => {
                self.on_account_storage_after_set_item(process).map(|_| TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::AccountStorageBeforeSetMapItem => Ok(TransactionEventHandling::Handled(Vec::new())),
            TransactionEvent::AccountStorageAfterSetMapItem => {
                self.on_account_storage_after_set_map_item(process).map(|_| TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::AccountBeforeIncrementNonce => {
                Ok(TransactionEventHandling::Handled(Vec::new()))
            },
            TransactionEvent::AccountAfterIncrementNonce => {
                self.on_account_after_increment_nonce().map(|_| TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::AccountPushProcedureIndex => {
                self.on_account_push_procedure_index(process).map(TransactionEventHandling::Handled)
            },

            TransactionEvent::NoteBeforeCreated => Ok(TransactionEventHandling::Handled(Vec::new())),
            TransactionEvent::NoteAfterCreated => self.on_note_after_created(process).map(|_| TransactionEventHandling::Handled(Vec::new())),

            TransactionEvent::NoteBeforeAddAsset => self.on_note_before_add_asset(process).map(|_| TransactionEventHandling::Handled(Vec::new())),
            TransactionEvent::NoteAfterAddAsset => Ok(TransactionEventHandling::Handled(Vec::new())),

            TransactionEvent::AuthRequest => {
                let pub_key_hash = process.get_stack_word(0);
                let message = process.get_stack_word(1);
                let signature_key = Hasher::merge(&[pub_key_hash, message]);

                let signature_opt = process
                    .advice_provider()
                    .get_mapped_values(&signature_key)
                    .map(|slice| slice.to_vec());
                let commitments_opt = process.advice_provider().get_mapped_values(&message).map(|slice| slice.to_vec());


                Ok(TransactionEventHandling::Unhandled(TransactionEventData::AuthRequest { pub_key_hash, message, signature_key, signature_opt, commitments_opt }))
            },

            TransactionEvent::PrologueStart => {
                self.tx_progress.start_prologue(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            },
            TransactionEvent::PrologueEnd => {
                self.tx_progress.end_prologue(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::NotesProcessingStart => {
                self.tx_progress.start_notes_processing(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            },
            TransactionEvent::NotesProcessingEnd => {
                self.tx_progress.end_notes_processing(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::NoteExecutionStart => {
                let note_id = Self::get_current_note_id(process)?.expect(
                    "Note execution interval measurement is incorrect: check the placement of the start and the end of the interval",
                );
                self.tx_progress.start_note_execution(process.clk(), note_id);
                Ok(TransactionEventHandling::Handled(Vec::new()))
            },
            TransactionEvent::NoteExecutionEnd => {
                self.tx_progress.end_note_execution(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::TxScriptProcessingStart => {
                self.tx_progress.start_tx_script_processing(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            }
            TransactionEvent::TxScriptProcessingEnd => {
                self.tx_progress.end_tx_script_processing(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            }

            TransactionEvent::EpilogueStart => {
                self.tx_progress.start_epilogue(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            }
            TransactionEvent::EpilogueEnd => {
                self.tx_progress.end_epilogue(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            }
            TransactionEvent::LinkMapSetEvent => {
                return LinkMap::handle_set_event(process).map(TransactionEventHandling::Handled);
            },
            TransactionEvent::LinkMapGetEvent => {
                return LinkMap::handle_get_event(process).map(TransactionEventHandling::Handled);
            },
            TransactionEvent::Unauthorized => {
              // Note: This always returns an error to abort the transaction.
              Err(self.on_unauthorized(process))
            }
        }
        .map_err(EventError::from)?;

        Ok(advice_mutations)
    }

    /// Pushes a signature to the advice stack as a response to the `AuthRequest` event.
    ///
    /// The signature is fetched from the advice map and if it is not present, an error is returned.
    pub fn on_signature_requested(
        &mut self,
        process: &ProcessState,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        let pub_key = process.get_stack_word(0);
        let msg = process.get_stack_word(1);

        let signature_key = Hasher::merge(&[pub_key, msg]);

        let signature = process
            .advice_provider()
            .get_mapped_values(&signature_key)
            .ok_or_else(|| TransactionKernelError::MissingAuthenticator)?
            .to_vec();

        Ok(vec![AdviceMutation::ExtendStack { values: signature }])
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
        process: &ProcessState,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        let proc_idx = self.acct_procedure_index_map.get_proc_index(process)?;
        Ok(vec![AdviceMutation::ExtendStack { values: vec![Felt::from(proc_idx)] }])
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

    /// Aborts the transaction by extracting the
    /// [`TransactionSummary`](miden_objects::transaction::TransactionSummary) from the stack and
    /// returns it in an error.
    ///
    /// Expected stack state:
    ///
    /// ```text
    /// [SALT, OUTPUT_NOTES_COMMITMENT, INPUT_NOTES_COMMITMENT, ACCOUNT_DELTA_COMMITMENT]
    /// ```
    fn on_unauthorized(&self, process: &ProcessState) -> TransactionKernelError {
        let account_delta_commitment = process.get_stack_word(3);
        let input_notes_commitment = process.get_stack_word(2);
        let output_notes_commitment = process.get_stack_word(1);
        let salt = process.get_stack_word(0);

        TransactionKernelError::Unauthorized {
            account_delta_commitment,
            input_notes_commitment,
            output_notes_commitment,
            salt,
        }
    }

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Returns the ID of the currently executing input note, or None if the note execution hasn't
    /// started yet or has already ended.
    ///
    /// # Errors
    /// Returns an error if the address of the currently executing input note is invalid (e.g.,
    /// greater than `u32::MAX`).
    fn get_current_note_id(process: &ProcessState) -> Result<Option<NoteId>, EventError> {
        // get the note address in `Felt` or return `None` if the address hasn't been accessed
        // previously.
        let note_address_felt = match process.get_mem_value(process.ctx(), CURRENT_INPUT_NOTE_PTR) {
            Some(addr) => addr,
            None => return Ok(None),
        };
        // convert note address into u32
        let note_address = u32::try_from(note_address_felt).map_err(|_| {
            Box::<dyn Error + Send + Sync>::from(format!(
                "failed to convert {note_address_felt} into a memory address (u32)"
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

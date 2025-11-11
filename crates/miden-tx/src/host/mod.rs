mod account_delta_tracker;

use account_delta_tracker::AccountDeltaTracker;
mod storage_delta_tracker;

mod link_map;
pub use link_map::{LinkMap, MemoryViewer};

mod account_procedures;
pub use account_procedures::AccountProcedureIndexMap;

pub(crate) mod note_builder;
use miden_lib::StdLibrary;
use note_builder::OutputNoteBuilder;

mod kernel_process;
use kernel_process::TransactionKernelProcess;

mod script_mast_forest_store;
pub use script_mast_forest_store::ScriptMastForestStore;

mod tx_progress;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_lib::transaction::{EventId, TransactionEvent, TransactionEventError};
use miden_objects::account::{
    AccountCode,
    AccountDelta,
    AccountHeader,
    AccountId,
    AccountStorageHeader,
    PartialAccount,
    StorageMap,
    StorageSlotType,
};
use miden_objects::asset::{Asset, AssetVault, AssetVaultKey, FungibleAsset};
use miden_objects::note::{NoteId, NoteInputs, NoteMetadata, NoteRecipient, NoteScript};
use miden_objects::transaction::{
    InputNote,
    InputNotes,
    OutputNote,
    OutputNotes,
    TransactionMeasurements,
    TransactionSummary,
};
use miden_objects::vm::RowIndex;
use miden_objects::{Hasher, Word};
use miden_processor::{
    AdviceError,
    AdviceMutation,
    ContextId,
    EventError,
    EventHandlerRegistry,
    Felt,
    MastForest,
    MastForestStore,
    ProcessState,
};
pub use tx_progress::TransactionProgress;

use crate::auth::SigningInputs;
use crate::errors::{TransactionHostError, TransactionKernelError};

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

    /// The header of the account at the beginning of transaction execution.
    initial_account_header: AccountHeader,

    /// The storage header of the native account at the beginning of transaction execution.
    initial_account_storage_header: AccountStorageHeader,

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

    /// Handle the VM default events _before_ passing it to user defined ones.
    stdlib_handlers: EventHandlerRegistry,
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
        let stdlib_handlers = {
            let mut registry = EventHandlerRegistry::new();

            let stdlib = StdLibrary::default();
            for (event_id, handler) in stdlib.handlers() {
                registry
                    .register(event_id, handler)
                    .expect("There are no duplicates in the stdlibrary handlers");
            }
            registry
        };
        Self {
            mast_store,
            scripts_mast_store,
            initial_account_header: account.into(),
            initial_account_storage_header: account.storage().header().clone(),
            account_delta: AccountDeltaTracker::new(account),
            acct_procedure_index_map,
            output_notes: BTreeMap::default(),
            input_notes,
            tx_progress: TransactionProgress::default(),
            stdlib_handlers,
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

    /// Returns a reference to the initial account header of the native account, which represents
    /// the state at the beginning of the transaction.
    pub fn initial_account_header(&self) -> &AccountHeader {
        &self.initial_account_header
    }

    /// Returns a reference to the initial storage header of the native account, which represents
    /// the state at the beginning of the transaction.
    pub fn initial_account_storage_header(&self) -> &AccountStorageHeader {
        &self.initial_account_storage_header
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
    pub fn input_notes(&self) -> InputNotes<InputNote> {
        self.input_notes.clone()
    }

    /// Clones the inner [`OutputNoteBuilder`]s and returns the vector of created output notes that
    /// are tracked by this host.
    pub fn build_output_notes(&self) -> Vec<OutputNote> {
        self.output_notes.values().cloned().map(|builder| builder.build()).collect()
    }

    /// Consumes `self` and returns the account delta, output notes and transaction progress.
    pub fn into_parts(
        self,
    ) -> (AccountDelta, InputNotes<InputNote>, Vec<OutputNote>, TransactionProgress) {
        let output_notes = self.output_notes.into_values().map(|builder| builder.build()).collect();

        (
            self.account_delta.into_delta(),
            self.input_notes,
            output_notes,
            self.tx_progress,
        )
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Inserts an output note builder at the specified index.
    ///
    /// # Errors
    /// Returns an error if a note builder already exists at the given index.
    pub(super) fn insert_output_note_builder(
        &mut self,
        note_idx: usize,
        note_builder: OutputNoteBuilder,
    ) -> Result<(), TransactionKernelError> {
        if self.output_notes.contains_key(&note_idx) {
            return Err(TransactionKernelError::other(format!(
                "Attempted to create note builder for note index {} twice",
                note_idx
            )));
        }
        self.output_notes.insert(note_idx, note_builder);
        Ok(())
    }

    /// Returns a mutable reference to the [`AccountProcedureIndexMap`].
    pub fn load_foreign_account_code(
        &mut self,
        account_code: &AccountCode,
    ) -> Result<(), TransactionHostError> {
        self.acct_procedure_index_map.insert_code(account_code)
    }

    // EVENT HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Handles the given [`TransactionEvent`], for example by updating the account delta or pushing
    /// requested advice to the advice stack.
    pub(super) fn handle_event(
        &mut self,
        process: &ProcessState,
        event_id: EventId,
    ) -> Result<TransactionEventHandling, EventError> {
        if let Some(mutations) = self.stdlib_handlers.handle_event(event_id, process)? {
            return Ok(TransactionEventHandling::Handled(mutations));
        }

        let transaction_event = TransactionEvent::try_from(event_id).map_err(EventError::from)?;

        // Privileged events can only be emitted from the root context.
        if process.ctx() != ContextId::root() && transaction_event.is_privileged() {
            return Err(Box::new(TransactionEventError::NotRootContext(transaction_event)));
        }

        let advice_mutations = match transaction_event {
            TransactionEvent::AccountBeforeForeignLoad => {
                self.on_account_before_foreign_load(process)
            }

            TransactionEvent::AccountVaultBeforeAddAsset => {
                self.on_account_vault_before_add_or_remove_asset(process)
            },
            TransactionEvent::AccountVaultAfterAddAsset => {
                self.on_account_vault_after_add_asset(process).map(|_| TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::AccountVaultBeforeRemoveAsset => {
                self.on_account_vault_before_add_or_remove_asset(process)
            },
            TransactionEvent::AccountVaultAfterRemoveAsset => {
                self.on_account_vault_after_remove_asset(process).map(|_| TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::AccountVaultBeforeGetBalance => {
                self.on_account_vault_before_get_balance(process)
            },

            TransactionEvent::AccountVaultBeforeHasNonFungibleAsset => {
                self.on_account_vault_before_has_non_fungible_asset(process)
            }

            TransactionEvent::AccountStorageBeforeGetMapItem => {
                self.on_account_storage_before_get_map_item(process)
            }

            TransactionEvent::AccountStorageBeforeSetItem => Ok(TransactionEventHandling::Handled(Vec::new())),
            TransactionEvent::AccountStorageAfterSetItem => {
                self.on_account_storage_after_set_item(process).map(|_| TransactionEventHandling::Handled(Vec::new()))
            },

            TransactionEvent::AccountStorageBeforeSetMapItem => {
                self.on_account_storage_before_set_map_item(process)
            },
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
            TransactionEvent::NoteAfterCreated => self.on_note_after_created(process),

            TransactionEvent::NoteBeforeAddAsset => self.on_note_before_add_asset(process).map(|_| TransactionEventHandling::Handled(Vec::new())),
            TransactionEvent::NoteAfterAddAsset => Ok(TransactionEventHandling::Handled(Vec::new())),

            TransactionEvent::AuthRequest => self.on_auth_requested(process),

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
                let note_id = process.get_active_note_id()?.expect(
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
            TransactionEvent::EpilogueAuthProcStart => {
                self.tx_progress.start_auth_procedure(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            }
            TransactionEvent::EpilogueAuthProcEnd => {
                self.tx_progress.end_auth_procedure(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            }
            TransactionEvent::EpilogueAfterTxCyclesObtained => {
                self.tx_progress.epilogue_after_tx_cycles_obtained(process.clk());
                Ok(TransactionEventHandling::Handled(vec![]))
            }
            TransactionEvent::EpilogueBeforeTxFeeRemovedFromAccount => self.on_before_tx_fee_removed_from_account(process),
            TransactionEvent::EpilogueEnd => {
                self.tx_progress.end_epilogue(process.clk());
                Ok(TransactionEventHandling::Handled(Vec::new()))
            }
            TransactionEvent::LinkMapSet => {
                return LinkMap::handle_set_event(process).map(TransactionEventHandling::Handled);
            },
            TransactionEvent::LinkMapGet => {
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

    /// Extract all necessary data for requesting the data to access the foreign account that is
    /// being loaded.
    ///
    /// Expected stack state: `[event, account_id_prefix, account_id_suffix]`
    pub fn on_account_before_foreign_load(
        &self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let account_id_word = process.get_stack_word_be(1);
        let account_id =
            AccountId::try_from([account_id_word[3], account_id_word[2]]).map_err(|err| {
                TransactionKernelError::other_with_source(
                    "failed to convert account ID word into account ID",
                    err,
                )
            })?;

        Ok(TransactionEventHandling::Unhandled(TransactionEventData::ForeignAccount {
            account_id,
        }))
    }

    /// Pushes a signature to the advice stack as a response to the `AuthRequest` event.
    ///
    /// Expected stack state: `[event, MESSAGE, PUB_KEY]`
    ///
    /// The signature is fetched from the advice map using `hash(PUB_KEY, MESSAGE)` as the key. If
    /// not present in the advice map [`TransactionEventHandling::Unhandled`] is returned with the
    /// data required to request a signature from a
    /// [`TransactionAuthenticator`](crate::auth::TransactionAuthenticator).
    fn on_auth_requested(
        &self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let message = process.get_stack_word_be(1);
        let pub_key_hash = process.get_stack_word_be(5);
        let signature_key = Hasher::merge(&[pub_key_hash, message]);

        let tx_summary = self.build_tx_summary(process, message)?;

        if message != tx_summary.to_commitment() {
            return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                "transaction summary doesn't commit to the expected message".into(),
            ));
        }

        let signing_inputs = SigningInputs::TransactionSummary(Box::new(tx_summary));

        if let Some(signature) = process
            .advice_provider()
            .get_mapped_values(&signature_key)
            .map(|slice| slice.to_vec())
        {
            return Ok(TransactionEventHandling::Handled(vec![AdviceMutation::extend_stack(
                signature,
            )]));
        }

        // If the signature is not present in the advice map, extract the necessary data to handle
        // the event at a later point.
        Ok(TransactionEventHandling::Unhandled(TransactionEventData::AuthRequest {
            pub_key_hash,
            signing_inputs,
        }))
    }

    /// Aborts the transaction by building the
    /// [`TransactionSummary`](miden_objects::transaction::TransactionSummary) based on elements on
    /// the operand stack and advice map.
    ///
    /// Expected stack state:
    ///
    /// `[event, MESSAGE]`
    ///
    /// Expected advice map state:
    ///
    /// ```text
    /// MESSAGE -> [SALT, OUTPUT_NOTES_COMMITMENT, INPUT_NOTES_COMMITMENT, ACCOUNT_DELTA_COMMITMENT]
    /// ```
    fn on_unauthorized(&self, process: &ProcessState) -> TransactionKernelError {
        let msg = process.get_stack_word_be(1);

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

    /// Extracts all necessary data to handle
    /// [`TransactionEvent::EpilogueBeforeTxFeeRemovedFromAccount`].
    ///
    /// Expected stack state:
    ///
    /// ```text
    /// `[event, FEE_ASSET]`
    /// ```
    fn on_before_tx_fee_removed_from_account(
        &self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let fee_asset = process.get_stack_word_be(1);
        let fee_asset = FungibleAsset::try_from(fee_asset)
            .map_err(TransactionKernelError::FailedToConvertFeeAsset)?;

        Ok(TransactionEventHandling::Unhandled(
            TransactionEventData::TransactionFeeComputed { fee_asset },
        ))
    }

    /// Handles the note creation event by extracting note data from the stack and advice provider.
    ///
    /// If the recipient data and note script are present in the advice provider, creates a new
    /// [`OutputNoteBuilder`] and stores it in the `output_notes` field of this
    /// [`TransactionBaseHost`]. Otherwise, returns [`TransactionEventHandling::Unhandled`] to
    /// request the missing note script from the data store.
    ///
    /// Expected stack state: `[event, NOTE_METADATA, note_ptr, RECIPIENT, note_idx]`
    fn on_note_after_created(
        &mut self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let metadata_word = process.get_stack_word_be(1);
        let metadata = NoteMetadata::try_from(metadata_word)
            .map_err(TransactionKernelError::MalformedNoteMetadata)?;

        let recipient_digest = process.get_stack_word_be(6);
        let note_idx = process.get_stack_item(10).as_int() as usize;

        // try to read the full recipient from the advice provider
        let recipient = if process.has_advice_map_entry(recipient_digest) {
            let (inputs, script_root, serial_num) =
                process.read_note_recipient_info_from_adv_map(recipient_digest)?;

            if let Some(script_data) = process.advice_provider().get_mapped_values(&script_root) {
                let script = NoteScript::try_from(script_data).map_err(|source| {
                    TransactionKernelError::MalformedNoteScript {
                        data: script_data.to_vec(),
                        source,
                    }
                })?;

                Some(NoteRecipient::new(serial_num, script, inputs))
            } else {
                // we couldn't build the full recipient because script root was missing; return the
                // info that we did read so that we could request the script from the data store
                return Ok(TransactionEventHandling::Unhandled(TransactionEventData::NoteData {
                    note_idx,
                    metadata,
                    script_root,
                    recipient_digest,
                    note_inputs: inputs,
                    serial_num,
                }));
            }
        } else {
            None
        };

        let note_builder = OutputNoteBuilder::new(metadata, recipient_digest, recipient)?;
        self.insert_output_note_builder(note_idx, note_builder)?;

        Ok(TransactionEventHandling::Handled(Vec::new()))
    }

    /// Adds an asset at the top of the [OutputNoteBuilder] identified by the note pointer.
    ///
    /// Expected stack state: `[event, ASSET, note_ptr, num_of_assets, note_idx]`
    fn on_note_before_add_asset(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let stack = process.get_stack_state();
        //# => [ASSET, note_ptr, num_of_assets, note_idx]

        let note_idx = stack[7].as_int();
        assert!(note_idx < self.output_notes.len() as u64);
        let node_idx = note_idx as usize;

        let asset_word = process.get_stack_word_be(1);
        let asset = Asset::try_from(asset_word).map_err(|source| {
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
    /// Expected stack state: `[event, PROC_ROOT, ...]`
    fn on_account_push_procedure_index(
        &mut self,
        process: &ProcessState,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        let proc_idx = self.acct_procedure_index_map.get_proc_index(process)?;
        Ok(vec![AdviceMutation::extend_stack([Felt::from(proc_idx)])])
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
    /// Expected stack state: `[event, slot_ptr, VALUE]`
    pub fn on_account_storage_after_set_item(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let slot_ptr = process.get_stack_item(1);
        let new_value = process.get_stack_word_be(2);

        let (slot_name_id, slot_type, _old_value) = process.get_storage_slot(process, slot_ptr)?;

        let (slot_name, ..) = self
            .initial_account_storage_header()
            .find_slot_header_by_id(slot_name_id)
            .ok_or_else(|| {
                TransactionKernelError::other(format!(
                    "failed to find slot header with name ID {slot_name_id}"
                ))
            })?;
        let slot_name = slot_name.clone();

        if !slot_type.is_value() {
            return Err(TransactionKernelError::other(format!(
                "expected slot to be of type value, found {slot_type}"
            )));
        }

        self.account_delta.storage().set_item(slot_name.clone(), new_value);

        Ok(())
    }

    /// Checks if the necessary witness for accessing the map item is already in the merkle store,
    /// and if not, extracts all necessary data for requesting it.
    ///
    /// Expected stack state: `[event, slot_ptr, KEY]`
    pub fn on_account_storage_before_get_map_item(
        &self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let slot_ptr = process.get_stack_item(1);
        let map_key = process.get_stack_word_be(2);

        self.on_account_storage_before_get_or_set_map_item(slot_ptr, map_key, process)
    }

    /// Checks if the necessary witness for accessing the map item is already in the merkle store,
    /// and if not, extracts all necessary data for requesting it.
    ///
    /// Expected stack state: `[event, slot_ptr, KEY]`
    pub fn on_account_storage_before_set_map_item(
        &self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let slot_ptr = process.get_stack_item(1);
        let map_key = process.get_stack_word_be(2);

        self.on_account_storage_before_get_or_set_map_item(slot_ptr, map_key, process)
    }

    /// Checks if the necessary witness for accessing the map item is already in the merkle store,
    /// and if not, extracts all necessary data for requesting it.
    fn on_account_storage_before_get_or_set_map_item(
        &self,
        slot_ptr: Felt,
        map_key: Word,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let (slot_name_id, slot_type, current_map_root) =
            process.get_storage_slot(process, slot_ptr)?;

        if !slot_type.is_map() {
            return Err(TransactionKernelError::other(format!(
                "expected slot to be of type map, found {slot_type}"
            )));
        }

        let current_account_id = process.get_active_account_id()?;
        let hashed_map_key = StorageMap::hash_key(map_key);
        let leaf_index = StorageMap::hashed_map_key_to_leaf_index(hashed_map_key);

        if advice_provider_has_merkle_path::<{ StorageMap::DEPTH }>(
            process,
            current_map_root,
            leaf_index,
        )? {
            // If the merkle path is already in the store there is nothing to do.
            Ok(TransactionEventHandling::Handled(Vec::new()))
        } else {
            // For the native account we need to explicitly request the initial map root, while for
            // foreign accounts the current map root is always the initial one.
            let map_root = if current_account_id == self.initial_account_header().id() {
                // For native accounts, we have to request witnesses against the initial root
                // instead of the _current_ one, since the data store only has
                // witnesses for the initial one.
                let (_slot_name, slot_type, slot_value) = self
                    .initial_account_storage_header()
                    .find_slot_header_by_id(slot_name_id)
                    .ok_or_else(|| {
                        TransactionKernelError::other(format!(
                            "failed to find storage map with name {slot_name_id} in storage header"
                        ))
                    })?;
                if *slot_type != StorageSlotType::Map {
                    return Err(TransactionKernelError::other(format!(
                        "expected slot {slot_name_id} to be of type map"
                    )));
                }
                *slot_value
            } else {
                current_map_root
            };

            // If the merkle path is not in the store return the data to request it.
            Ok(TransactionEventHandling::Unhandled(
                TransactionEventData::AccountStorageMapWitness {
                    current_account_id,
                    map_root,
                    map_key,
                },
            ))
        }
    }

    /// Extracts information from the process state about the storage map being updated and
    /// records the latest values of this storage map.
    ///
    /// Expected stack state:
    /// ```text
    /// [event, slot_ptr, KEY, OLD_MAP_VALUE, NEW_VALUE]
    /// ```
    pub fn on_account_storage_after_set_map_item(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let slot_ptr = process.get_stack_item(1);
        let key = process.get_stack_word_be(2);
        let old_map_value = process.get_stack_word_be(6);
        let new_map_value = process.get_stack_word_be(10);

        let (slot_name_id, ..) = process.get_storage_slot(process, slot_ptr)?;

        let (slot_name, ..) = self
            .initial_account_storage_header()
            .find_slot_header_by_id(slot_name_id)
            .ok_or_else(|| {
                TransactionKernelError::other(format!(
                    "failed to resolve slot name ID {slot_name_id} to slot name"
                ))
            })?;
        let slot_name = slot_name.clone();

        self.account_delta
            .storage()
            .set_map_item(slot_name, key, old_map_value, new_map_value);

        Ok(())
    }

    // ACCOUNT VAULT UPDATE HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Extracts the asset that is being added to the account's vault from the process state and
    /// updates the appropriate fungible or non-fungible asset map.
    ///
    /// Expected stack state: `[event, ASSET, ...]`
    pub fn on_account_vault_after_add_asset(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let asset: Asset = process.get_stack_word_be(1).try_into().map_err(|source| {
            TransactionKernelError::MalformedAssetInEventHandler {
                handler: "on_account_vault_after_add_asset",
                source,
            }
        })?;

        self.account_delta
            .vault_delta_mut()
            .add_asset(asset)
            .map_err(TransactionKernelError::AccountDeltaAddAssetFailed)?;
        Ok(())
    }

    /// Checks if the necessary witness for accessing the asset is already in the merkle store,
    /// and if not, extracts all necessary data for requesting it.
    ///
    /// Expected stack state: `[event, ASSET, account_vault_root_ptr]`
    pub fn on_account_vault_before_add_or_remove_asset(
        &self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let asset_word = process.get_stack_word_be(1);
        let asset = Asset::try_from(asset_word).map_err(|source| {
            TransactionKernelError::MalformedAssetInEventHandler {
                handler: "on_account_vault_before_add_or_remove_asset",
                source,
            }
        })?;

        let vault_root_ptr = process.get_stack_item(5);
        let vault_root_ptr = u32::try_from(vault_root_ptr).map_err(|_err| {
            TransactionKernelError::other(format!(
                "vault root ptr should fit into a u32, but was {vault_root_ptr}"
            ))
        })?;
        let current_vault_root = process
            .get_mem_word(process.ctx(), vault_root_ptr)
            .map_err(|_err| {
                TransactionKernelError::other(format!(
                    "vault root ptr {vault_root_ptr} is not word-aligned"
                ))
            })?
            .ok_or_else(|| {
                TransactionKernelError::other(format!(
                    "vault root ptr {vault_root_ptr} was not initialized"
                ))
            })?;

        self.on_account_vault_asset_accessed(process, asset.vault_key(), current_vault_root)
    }

    /// Extracts the asset that is being removed from the account's vault from the process state
    /// and updates the appropriate fungible or non-fungible asset map.
    ///
    /// Expected stack state: `[event, ASSET, ...]`
    pub fn on_account_vault_after_remove_asset(
        &mut self,
        process: &ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let asset: Asset = process.get_stack_word_be(1).try_into().map_err(|source| {
            TransactionKernelError::MalformedAssetInEventHandler {
                handler: "on_account_vault_after_remove_asset",
                source,
            }
        })?;

        self.account_delta
            .vault_delta_mut()
            .remove_asset(asset)
            .map_err(TransactionKernelError::AccountDeltaRemoveAssetFailed)?;
        Ok(())
    }

    /// Checks if the necessary witness for accessing the asset is already in the merkle store,
    /// and if not, extracts all necessary data for requesting it.
    ///
    /// Expected stack state: `[event, faucet_id_prefix, faucet_id_suffix, vault_root_ptr]`
    pub fn on_account_vault_before_get_balance(
        &self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let stack_top = process.get_stack_word_be(1);
        let faucet_id = AccountId::try_from([stack_top[3], stack_top[2]]).map_err(|err| {
            TransactionKernelError::other_with_source(
                "failed to convert faucet ID word into faucet ID",
                err,
            )
        })?;
        let vault_root_ptr = stack_top[1];
        let vault_root = process.get_vault_root(vault_root_ptr)?;

        let vault_key = AssetVaultKey::from_account_id(faucet_id).ok_or_else(|| {
            TransactionKernelError::other(format!(
                "provided faucet ID {faucet_id} is not valid for fungible assets"
            ))
        })?;
        self.on_account_vault_asset_accessed(process, vault_key, vault_root)
    }

    /// Checks if the necessary witness for accessing the asset is already in the merkle store,
    /// and if not, extracts all necessary data for requesting it.
    ///
    /// Expected stack state: `[event, ASSET, vault_root_ptr]`
    pub fn on_account_vault_before_has_non_fungible_asset(
        &self,
        process: &ProcessState,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let asset_word = process.get_stack_word_be(1);
        let asset = Asset::try_from(asset_word).map_err(|err| {
            TransactionKernelError::other_with_source("provided asset is not a valid asset", err)
        })?;

        let vault_root_ptr = process.get_stack_item(5);
        let vault_root = process.get_vault_root(vault_root_ptr)?;

        self.on_account_vault_asset_accessed(process, asset.vault_key(), vault_root)
    }

    /// Checks if the necessary witness for accessing the provided asset is already in the merkle
    /// store, and if not, extracts all necessary data for requesting it.
    fn on_account_vault_asset_accessed(
        &self,
        process: &ProcessState,
        vault_key: AssetVaultKey,
        current_vault_root: Word,
    ) -> Result<TransactionEventHandling, TransactionKernelError> {
        let leaf_index = Felt::new(vault_key.to_leaf_index().value());
        let active_account_id = process.get_active_account_id()?;

        // Note that we check whether a merkle path for the current vault root is present, not
        // necessarily for the root we are going to request. This is because the end goal is to
        // enable access to an asset against the current vault root, and so if this
        // condition is already satisfied, there is nothing to request.
        if advice_provider_has_merkle_path::<{ AssetVault::DEPTH }>(
            process,
            current_vault_root,
            leaf_index,
        )? {
            // If the merkle path is already in the store there is nothing to do.
            Ok(TransactionEventHandling::Handled(Vec::new()))
        } else {
            // For the native account we need to explicitly request the initial vault root, while
            // for foreign accounts the current vault root is always the initial one.
            let vault_root = if active_account_id == self.initial_account_header().id() {
                self.initial_account_header().vault_root()
            } else {
                current_vault_root
            };

            // If the merkle path is not in the store return the data to request it.
            Ok(TransactionEventHandling::Unhandled(
                TransactionEventData::AccountVaultAssetWitness {
                    current_account_id: active_account_id,
                    vault_root,
                    asset_key: vault_key,
                },
            ))
        }
    }

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Builds a [TransactionSummary] by extracting data from the advice provider and validating
    /// commitments against the host's state.
    pub(crate) fn build_tx_summary(
        &self,
        process: &ProcessState,
        msg: Word,
    ) -> Result<TransactionSummary, TransactionKernelError> {
        let Some(commitments) = process.advice_provider().get_mapped_values(&msg) else {
            return Err(TransactionKernelError::TransactionSummaryConstructionFailed(
                "Expected message to exist in advice provider".into(),
            ));
        };

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
}

impl<'store, STORE> TransactionBaseHost<'store, STORE> {
    /// Returns the underlying store of the base host.
    pub fn store(&self) -> &'store STORE {
        self.mast_store
    }
}

// TRANSACTION EVENT HANDLING
// ================================================================================================

/// Indicates whether a [`TransactionEvent`] was handled or not.
///
/// If it is unhandled, the necessary data to handle it is returned.
#[derive(Debug)]
pub(super) enum TransactionEventHandling {
    Unhandled(TransactionEventData),
    Handled(Vec<AdviceMutation>),
}

/// The data necessary to handle an [`TransactionEvent`].
#[derive(Debug, Clone)]
pub(super) enum TransactionEventData {
    /// The data necessary to handle an auth request.
    AuthRequest {
        /// The hash of the public key for which a signature was requested.
        pub_key_hash: Word,
        /// The signing inputs that summarize what is being signed. The commitment to these inputs
        /// is the message that is being signed.
        signing_inputs: SigningInputs,
    },
    /// The data necessary to handle a transaction fee computed event.
    TransactionFeeComputed {
        /// The fee asset extracted from the stack.
        fee_asset: FungibleAsset,
    },
    /// The data necessary to request a foreign account's data from the data store.
    ForeignAccount {
        /// The foreign account's ID.
        account_id: AccountId,
    },
    /// The data necessary to request an asset witness from the data store.
    AccountVaultAssetWitness {
        /// The account ID for whose vault a witness is requested.
        current_account_id: AccountId,
        /// The vault root identifying the asset vault from which a witness is requested.
        vault_root: Word,
        /// The asset for which a witness is requested.
        asset_key: AssetVaultKey,
    },
    /// The data necessary to request a storage map witness from the data store.
    AccountStorageMapWitness {
        /// The account ID for whose storage a witness is requested.
        current_account_id: AccountId,
        /// The root of the storage map in the account at the beginning of the transaction.
        map_root: Word,
        /// The raw map key for which a witness is requested.
        map_key: Word,
    },
    /// The data necessary to request a note script from the data store.
    NoteData {
        /// The note index extracted from the stack.
        note_idx: usize,
        /// The note metadata extracted from the stack.
        metadata: NoteMetadata,
        /// The root of the note script being requested.
        script_root: Word,
        /// The recipient digest extracted from the stack.
        recipient_digest: Word,
        /// The note inputs extracted from the advice provider.
        note_inputs: NoteInputs,
        /// The serial number extracted from the advice provider.
        serial_num: Word,
    },
}

// HELPER FUNCTIONS
// ================================================================================================

/// Returns `true` if the advice provider has a merkle path for the provided root and leaf
/// index, `false` otherwise.
fn advice_provider_has_merkle_path<const TREE_DEPTH: u8>(
    process: &ProcessState,
    root: Word,
    leaf_index: Felt,
) -> Result<bool, TransactionKernelError> {
    match process
        .advice_provider()
        .get_merkle_path(root, Felt::from(TREE_DEPTH), leaf_index)
    {
        // Merkle path is already in the store; consider the event handled.
        Ok(_) => Ok(true),
        // This means the merkle path is missing in the advice provider.
        Err(AdviceError::MerkleStoreLookupFailed(_)) => Ok(false),
        // We should never encounter this as long as our inputs to get_merkle_path are correct.
        Err(err) => Err(TransactionKernelError::other_with_source(
            "unexpected get_merkle_path error",
            err,
        )),
    }
}

/// Extracts a word from a slice of field elements.
#[inline(always)]
fn extract_word(commitments: &[Felt], start: usize) -> Word {
    Word::from([
        commitments[start],
        commitments[start + 1],
        commitments[start + 2],
        commitments[start + 3],
    ])
}

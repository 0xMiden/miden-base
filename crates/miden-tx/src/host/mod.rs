mod account_delta_tracker;

use account_delta_tracker::AccountDeltaTracker;
mod storage_delta_tracker;

mod link_map;
pub use link_map::{LinkMap, MemoryViewer};

mod account_procedures;
pub use account_procedures::AccountProcedureIndexMap;

pub(crate) mod note_builder;
use miden_lib::StdLibrary;
use miden_lib::transaction::EventId;
use note_builder::OutputNoteBuilder;

mod kernel_process;
use kernel_process::TransactionKernelProcess;

mod script_mast_forest_store;
pub use script_mast_forest_store::ScriptMastForestStore;

mod tx_progress;

mod tx_event;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_objects::Word;
use miden_objects::account::{
    AccountCode,
    AccountDelta,
    AccountHeader,
    AccountStorageHeader,
    PartialAccount,
};
use miden_objects::asset::Asset;
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
use miden_processor::{
    AdviceMutation,
    EventError,
    EventHandlerRegistry,
    Felt,
    MastForest,
    MastForestStore,
    ProcessState,
};
pub(crate) use tx_event::TransactionEvent;
pub use tx_progress::TransactionProgress;

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

    /// Returns a mutable reference to the `tx_progress` field of this transaction host.
    pub fn tx_progress_mut(&mut self) -> &mut TransactionProgress {
        &mut self.tx_progress
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

    pub fn handle_stdlib_events(
        &self,
        process: &ProcessState,
    ) -> Result<Option<Vec<AdviceMutation>>, EventError> {
        let event_id = EventId::from_felt(process.get_stack_item(0));
        if let Some(mutations) = self.stdlib_handlers.handle_event(event_id, process)? {
            Ok(Some(mutations))
        } else {
            Ok(None)
        }
    }

    /// Pushes a signature to the advice stack as a response to the `AuthRequest` event.
    ///
    /// Expected stack state: `[event, MESSAGE, PUB_KEY]`
    ///
    /// The signature is fetched from the advice map using `hash(PUB_KEY, MESSAGE)` as the key. If
    /// not present in the advice map [`TransactionEventHandling::Unhandled`] is returned with the
    /// data required to request a signature from a
    /// [`TransactionAuthenticator`](crate::auth::TransactionAuthenticator).
    pub fn on_auth_requested(&self, signature: Vec<Felt>) -> Vec<AdviceMutation> {
        vec![AdviceMutation::extend_stack(signature)]
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
    pub fn on_unauthorized(
        &self,
        message: Word,
        salt: Word,
        output_notes_commitment: Word,
        input_notes_commitment: Word,
        account_delta_commitment: Word,
    ) -> TransactionKernelError {
        let tx_summary = match self.build_tx_summary(
            salt,
            output_notes_commitment,
            input_notes_commitment,
            account_delta_commitment,
        ) {
            Ok(tx_summary) => tx_summary,
            Err(err) => return err,
        };

        if message != tx_summary.to_commitment() {
            return TransactionKernelError::TransactionSummaryConstructionFailed(
                "transaction summary doesn't commit to the expected message".into(),
            );
        }

        TransactionKernelError::Unauthorized(Box::new(tx_summary))
    }

    /// Handles the note creation event by extracting note data from the stack and advice provider.
    ///
    /// If the recipient data and note script are present in the advice provider, creates a new
    /// [`OutputNoteBuilder`] and stores it in the `output_notes` field of this
    /// [`TransactionBaseHost`]. Otherwise, returns [`TransactionEventHandling::Unhandled`] to
    /// request the missing note script from the data store.
    ///
    /// Expected stack state: `[event, NOTE_METADATA, note_ptr, RECIPIENT, note_idx]`
    pub fn on_note_after_created(
        &mut self,
        note_idx: usize,
        metadata: NoteMetadata,
        recipient_digest: Word,
        note_script: Option<NoteScript>,
        recipient_data: Option<(Word, Word, NoteInputs)>,
    ) -> Result<Option<(Word, Word, NoteInputs)>, TransactionKernelError> {
        let recipient = match (note_script, recipient_data) {
            // If recipient data is none, there is no point in requesting the script.
            (_, None) => None,
            // If the script is missing, return the recipient data so the script can be requested.
            (None, recipient_data @ Some(_)) => return Ok(recipient_data),
            // If both are present, we can build the recipient directly.
            (Some(note_script), Some((serial_num, _script_root, note_inputs))) => {
                Some(NoteRecipient::new(serial_num, note_script, note_inputs))
            },
        };

        let note_builder = OutputNoteBuilder::new(metadata, recipient_digest, recipient)?;
        self.insert_output_note_builder(note_idx, note_builder)?;

        Ok(None)
    }

    /// Adds an asset to the output note identified by the note index.
    pub fn on_note_before_add_asset(
        &mut self,
        note_idx: usize,
        asset: Asset,
    ) -> Result<(), TransactionKernelError> {
        let note_builder = self.output_notes.get_mut(&note_idx).ok_or_else(|| {
            TransactionKernelError::other(format!("failed to find output note {note_idx}"))
        })?;

        note_builder.add_asset(asset)?;

        Ok(())
    }

    /// Loads the index of the procedure root onto the advice stack.
    ///
    /// Expected stack state: `[event, PROC_ROOT, ...]`
    pub fn on_account_push_procedure_index(
        &mut self,
        code_commitment: Word,
        procedure_root: Word,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        let proc_idx =
            self.acct_procedure_index_map.get_proc_index(code_commitment, procedure_root)?;
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

    /// Tracks the insertion of an item in the account delta.
    pub fn on_account_storage_after_set_item(
        &mut self,
        slot_index: u8,
        current_slot_value: Word,
        new_slot_value: Word,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        self.account_delta
            .storage()
            .set_item(slot_index, current_slot_value, new_slot_value);

        Ok(Vec::new())
    }

    // /// Checks if the necessary witness for accessing the map item is already in the merkle
    // store, /// and if not, extracts all necessary data for requesting it.
    // ///
    // /// Expected stack state: `[event, KEY, ROOT, index]`
    // pub fn on_account_storage_before_get_map_item(
    //     &self,
    //     process: &ProcessState,
    // ) -> Result<TransactionEventHandling, TransactionKernelError> {
    //     let map_key = process.get_stack_word_be(1);
    //     let current_map_root = process.get_stack_word_be(5);
    //     let slot_index = process.get_stack_item(9);

    //     self.on_account_storage_before_get_or_set_map_item(
    //         slot_index,
    //         current_map_root,
    //         map_key,
    //         process,
    //     )
    // }

    // /// Checks if the necessary witness for accessing the map item is already in the merkle
    // store, /// and if not, extracts all necessary data for requesting it.
    // ///
    // /// Expected stack state: `[event, index, KEY, NEW_VALUE, OLD_ROOT]`
    // pub fn on_account_storage_before_set_map_item(
    //     &self,
    //     process: &ProcessState,
    // ) -> Result<TransactionEventHandling, TransactionKernelError> {
    //     let slot_index = process.get_stack_item(1);
    //     let map_key = process.get_stack_word_be(2);
    //     let current_map_root = process.get_stack_word_be(10);

    //     self.on_account_storage_before_get_or_set_map_item(
    //         slot_index,
    //         current_map_root,
    //         map_key,
    //         process,
    //     )
    // }

    // /// Checks if the necessary witness for accessing the map item is already in the merkle
    // store, /// and if not, extracts all necessary data for requesting it.
    // fn on_account_storage_before_get_or_set_map_item(
    //     &self,
    //     slot_index: Felt,
    //     current_map_root: Word,
    //     map_key: Word,
    //     process: &ProcessState,
    // ) -> Result<TransactionEventHandling, TransactionKernelError> {
    //     let current_account_id = process.get_active_account_id()?;
    //     let hashed_map_key = StorageMap::hash_key(map_key);
    //     let leaf_index = StorageMap::hashed_map_key_to_leaf_index(hashed_map_key);

    //     if advice_provider_has_merkle_path::<{ StorageMap::DEPTH }>(
    //         process,
    //         current_map_root,
    //         leaf_index,
    //     )? {
    //         // If the merkle path is already in the store there is nothing to do.
    //         Ok(TransactionEventHandling::Handled(Vec::new()))
    //     } else {
    //         // For the native account we need to explicitly request the initial map root, while
    // for         // foreign accounts the current map root is always the initial one.
    //         let map_root = if current_account_id == self.initial_account_header().id() {
    //             // For native accounts, we have to request witnesses against the initial root
    //             // instead of the _current_ one, since the data store only has
    //             // witnesses for initial one.
    //             let (slot_type, slot_value) = self
    //                 .initial_account_storage_header()
    //                 // Slot index should always fit into a usize.
    //                 .slot(slot_index.as_int() as usize)
    //                 .map_err(|err| {
    //                     TransactionKernelError::other_with_source(
    //                         "failed to access storage map in storage header",
    //                         err,
    //                     )
    //                 })?;
    //             if *slot_type != StorageSlotType::Map {
    //                 return Err(TransactionKernelError::other(format!(
    //                     "expected map slot type at slot index {slot_index}"
    //                 )));
    //             }
    //             *slot_value
    //         } else {
    //             current_map_root
    //         };

    //         // If the merkle path is not in the store return the data to request it.
    //         Ok(TransactionEventHandling::Unhandled(
    //             TransactionEvent::AccountStorageMapWitness {
    //                 current_account_id,
    //                 map_root,
    //                 map_key,
    //             },
    //         ))
    //     }
    // }

    /// Extracts information from the process state about the storage map being updated and
    /// records the latest values of this storage map.
    ///
    /// Expected stack state: `[event, slot_index, KEY, PREV_MAP_VALUE, NEW_MAP_VALUE]`
    pub fn on_account_storage_after_set_map_item(
        &mut self,
        slot_index: u8,
        key: Word,
        prev_map_value: Word,
        new_map_value: Word,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        self.account_delta
            .storage()
            .set_map_item(slot_index, key, prev_map_value, new_map_value);

        Ok(Vec::new())
    }

    // ACCOUNT VAULT UPDATE HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Extracts the asset that is being added to the account's vault from the process state and
    /// updates the appropriate fungible or non-fungible asset map.
    ///
    /// Expected stack state: `[event, ASSET, ...]`
    pub fn on_account_vault_after_add_asset(
        &mut self,
        asset: Asset,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        self.account_delta
            .vault_delta_mut()
            .add_asset(asset)
            .map_err(TransactionKernelError::AccountDeltaAddAssetFailed)?;

        Ok(Vec::new())
    }

    // /// Checks if the necessary witness for accessing the asset is already in the merkle store,
    // /// and if not, extracts all necessary data for requesting it.
    // ///
    // /// Expected stack state: `[event, ASSET, account_vault_root_ptr]`
    // pub fn on_account_vault_before_add_or_remove_asset(
    //     &self,
    //     process: &ProcessState,
    // ) -> Result<TransactionEventHandling, TransactionKernelError> {
    //     let asset_word = process.get_stack_word_be(1);
    //     let asset = Asset::try_from(asset_word).map_err(|source| {
    //         TransactionKernelError::MalformedAssetInEventHandler {
    //             handler: "on_account_vault_before_add_or_remove_asset",
    //             source,
    //         }
    //     })?;

    //     let vault_root_ptr = process.get_stack_item(5);
    //     let vault_root_ptr = u32::try_from(vault_root_ptr).map_err(|_err| {
    //         TransactionKernelError::other(format!(
    //             "vault root ptr should fit into a u32, but was {vault_root_ptr}"
    //         ))
    //     })?;
    //     let current_vault_root = process
    //         .get_mem_word(process.ctx(), vault_root_ptr)
    //         .map_err(|_err| {
    //             TransactionKernelError::other(format!(
    //                 "vault root ptr {vault_root_ptr} is not word-aligned"
    //             ))
    //         })?
    //         .ok_or_else(|| {
    //             TransactionKernelError::other(format!(
    //                 "vault root ptr {vault_root_ptr} was not initialized"
    //             ))
    //         })?;

    //     self.on_account_vault_asset_accessed(process, asset.vault_key(), current_vault_root)
    // }

    /// Tracks the removal of an asset in the account delta.
    pub fn on_account_vault_after_remove_asset(
        &mut self,
        asset: Asset,
    ) -> Result<Vec<AdviceMutation>, TransactionKernelError> {
        self.account_delta
            .vault_delta_mut()
            .remove_asset(asset)
            .map_err(TransactionKernelError::AccountDeltaRemoveAssetFailed)?;

        Ok(Vec::new())
    }

    // /// Checks if the necessary witness for accessing the asset is already in the merkle store,
    // /// and if not, extracts all necessary data for requesting it.
    // ///
    // /// Expected stack state: `[event, faucet_id_prefix, faucet_id_suffix, vault_root_ptr]`
    // pub fn on_account_vault_before_get_balance(
    //     &self,
    //     process: &ProcessState,
    // ) -> Result<TransactionEventHandling, TransactionKernelError> {
    //     let stack_top = process.get_stack_word_be(1);
    //     let faucet_id = AccountId::try_from([stack_top[3], stack_top[2]]).map_err(|err| {
    //         TransactionKernelError::other_with_source(
    //             "failed to convert faucet ID word into faucet ID",
    //             err,
    //         )
    //     })?;
    //     let vault_root_ptr = stack_top[1];
    //     let vault_root = process.get_vault_root(vault_root_ptr)?;

    //     let vault_key = AssetVaultKey::from_account_id(faucet_id).ok_or_else(|| {
    //         TransactionKernelError::other(format!(
    //             "provided faucet ID {faucet_id} is not valid for fungible assets"
    //         ))
    //     })?;
    //     self.on_account_vault_asset_accessed(process, vault_key, vault_root)
    // }

    // /// Checks if the necessary witness for accessing the asset is already in the merkle store,
    // /// and if not, extracts all necessary data for requesting it.
    // ///
    // /// Expected stack state: `[event, ASSET, vault_root_ptr]`
    // pub fn on_account_vault_before_has_non_fungible_asset(
    //     &self,
    //     process: &ProcessState,
    // ) -> Result<TransactionEventHandling, TransactionKernelError> {
    //     let asset_word = process.get_stack_word_be(1);
    //     let asset = Asset::try_from(asset_word).map_err(|err| {
    //         TransactionKernelError::other_with_source("provided asset is not a valid asset", err)
    //     })?;

    //     let vault_root_ptr = process.get_stack_item(5);
    //     let vault_root = process.get_vault_root(vault_root_ptr)?;

    //     self.on_account_vault_asset_accessed(process, asset.vault_key(), vault_root)
    // }

    // /// Checks if the necessary witness for accessing the provided asset is already in the merkle
    // /// store, and if not, extracts all necessary data for requesting it.
    // fn on_account_vault_asset_accessed(
    //     &self,
    //     process: &ProcessState,
    //     vault_key: AssetVaultKey,
    //     current_vault_root: Word,
    // ) -> Result<TransactionEventHandling, TransactionKernelError> {
    //     let leaf_index = Felt::new(vault_key.to_leaf_index().value());
    //     let active_account_id = process.get_active_account_id()?;

    //     // Note that we check whether a merkle path for the current vault root is present, not
    //     // necessarily for the root we are going to request. This is because the end goal is to
    //     // enable access to an asset against the current vault root, and so if this
    //     // condition is already satisfied, there is nothing to request.
    //     if advice_provider_has_merkle_path::<{ AssetVault::DEPTH }>(
    //         process,
    //         current_vault_root,
    //         leaf_index,
    //     )? {
    //         // If the merkle path is already in the store there is nothing to do.
    //         Ok(TransactionEventHandling::Handled(Vec::new()))
    //     } else {
    //         // For the native account we need to explicitly request the initial vault root, while
    //         // for foreign accounts the current vault root is always the initial one.
    //         let vault_root = if active_account_id == self.initial_account_header().id() {
    //             self.initial_account_header().vault_root()
    //         } else {
    //             current_vault_root
    //         };

    //         // If the merkle path is not in the store return the data to request it.
    //         Ok(TransactionEventHandling::Unhandled(TransactionEvent::AccountVaultAssetAccess {
    //             current_account_id: active_account_id,
    //             vault_root,
    //             asset_key: vault_key,
    //         }))
    //     }
    // }

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Builds a [TransactionSummary] by extracting data from the advice provider and validating
    /// commitments against the host's state.
    pub(crate) fn build_tx_summary(
        &self,
        salt: Word,
        output_notes_commitment: Word,
        input_notes_commitment: Word,
        account_delta_commitment: Word,
    ) -> Result<TransactionSummary, TransactionKernelError> {
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

// HELPER FUNCTIONS
// ================================================================================================

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

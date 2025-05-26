use alloc::{
    boxed::Box,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    vec::Vec,
};

use miden_lib::transaction::{
    TransactionEvent, TransactionEventError, TransactionKernelError,
    memory::{CURRENT_INPUT_NOTE_PTR, NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR},
};
use miden_objects::{
    Digest, Hasher,
    account::{AccountDelta, AccountHeader},
    assembly::mast::MastNodeExt,
    asset::Asset,
    note::NoteId,
    transaction::{OutputNote, TransactionMeasurements},
    vm::RowIndex,
};
use vm_processor::{
    AdviceProvider, AdviceSource, ContextId, ErrorContext, ExecutionError, Felt, Host, MastForest,
    MastForestStore, MemoryError, ProcessState,
};

mod account_delta_tracker;
use account_delta_tracker::AccountDeltaTracker;

mod account_procedures;
pub use account_procedures::AccountProcedureIndexMap;

mod note_builder;
use note_builder::OutputNoteBuilder;

mod tx_progress;
pub use tx_progress::TransactionProgress;

use crate::{auth::TransactionAuthenticator, errors::TransactionHostError};

// TRANSACTION HOST
// ================================================================================================

/// Transaction host is responsible for handling [Host] requests made by a transaction kernel.
///
/// Transaction hosts are created on a per-transaction basis. That is, a transaction host is meant
/// to support execution of a single transaction and is discarded after the transaction finishes
/// execution.
pub struct TransactionHost<A> {
    /// Advice provider which is used to provide non-deterministic inputs to the transaction
    /// runtime.
    adv_provider: A,

    /// MAST store which contains the code required to execute the transaction.
    mast_store: Arc<dyn MastForestStore>,

    /// Account state changes accumulated during transaction execution.
    ///
    /// This field is updated by the [TransactionHost::on_event()] handler.
    account_delta: AccountDeltaTracker,

    /// A map of the procedure MAST roots to the corresponding procedure indices for all the
    /// account codes involved in the transaction (for native and foreign accounts alike).
    acct_procedure_index_map: AccountProcedureIndexMap,

    /// The list of notes created while executing a transaction stored as note_ptr |-> note_builder
    /// map.
    output_notes: BTreeMap<usize, OutputNoteBuilder>,

    /// Serves signature generation requests from the transaction runtime for signatures which are
    /// not present in the `generated_signatures` field.
    authenticator: Option<Arc<dyn TransactionAuthenticator>>,

    /// Contains previously generated signatures (as a message |-> signature map) required for
    /// transaction execution.
    ///
    /// If a required signature is not present in this map, the host will attempt to generate the
    /// signature using the transaction authenticator.
    generated_signatures: BTreeMap<Digest, Vec<Felt>>,

    /// Tracks the number of cycles for each of the transaction execution stages.
    ///
    /// This field is updated by the [TransactionHost::on_trace()] handler.
    tx_progress: TransactionProgress,
}

impl<A: AdviceProvider> TransactionHost<A> {
    /// Returns a new [TransactionHost] instance with the provided [AdviceProvider].
    pub fn new(
        account: AccountHeader,
        adv_provider: A,
        mast_store: Arc<dyn MastForestStore>,
        authenticator: Option<Arc<dyn TransactionAuthenticator>>,
        mut foreign_account_code_commitments: BTreeSet<Digest>,
    ) -> Result<Self, TransactionHostError> {
        // currently, the executor/prover do not keep track of the code commitment of the native
        // account, so we add it to the set here
        foreign_account_code_commitments.insert(account.code_commitment());

        let proc_index_map =
            AccountProcedureIndexMap::new(foreign_account_code_commitments, &adv_provider)?;

        Ok(Self {
            adv_provider,
            mast_store,
            account_delta: AccountDeltaTracker::new(&account),
            acct_procedure_index_map: proc_index_map,
            output_notes: BTreeMap::default(),
            authenticator,
            tx_progress: TransactionProgress::default(),
            generated_signatures: BTreeMap::new(),
        })
    }

    /// Consumes `self` and returns the advice provider, account delta, output notes, generated
    /// signatures, and transaction progress.
    pub fn into_parts(
        self,
    ) -> (
        A,
        AccountDelta,
        Vec<OutputNote>,
        BTreeMap<Digest, Vec<Felt>>,
        TransactionProgress,
    ) {
        let output_notes = self.output_notes.into_values().map(|builder| builder.build()).collect();

        (
            self.adv_provider,
            self.account_delta.into_delta(),
            output_notes,
            self.generated_signatures,
            self.tx_progress,
        )
    }

    /// Returns a reference to the `tx_progress` field of this transaction host.
    pub fn tx_progress(&self) -> &TransactionProgress {
        &self.tx_progress
    }

    // EVENT HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [OutputNoteBuilder] from the data on the operand stack and stores it into the
    /// `output_notes` field of this [TransactionHost].
    ///
    /// Expected stack state: `[NOTE_METADATA, RECIPIENT, ...]`
    fn on_note_after_created(
        &mut self,
        process: ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let stack = process.get_stack_state();
        // # => [NOTE_METADATA]

        let note_idx: usize = stack[9].as_int() as usize;

        assert_eq!(note_idx, self.output_notes.len(), "note index mismatch");

        let note_builder = OutputNoteBuilder::new(stack, &self.adv_provider)?;

        self.output_notes.insert(note_idx, note_builder);

        Ok(())
    }

    /// Adds an asset at the top of the [OutputNoteBuilder] identified by the note pointer.
    ///
    /// Expected stack state: [ASSET, note_ptr, num_of_assets, note_idx]
    fn on_note_before_add_asset(
        &mut self,
        process: ProcessState,
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
        process: ProcessState,
        err_ctx: &ErrorContext<'_, impl MastNodeExt>,
    ) -> Result<(), TransactionKernelError> {
        let proc_idx = self.acct_procedure_index_map.get_proc_index(&process)?;
        self.adv_provider
            .push_stack(AdviceSource::Value(proc_idx.into()), err_ctx)
            .expect("failed to push value onto advice stack");
        Ok(())
    }

    /// Extracts the nonce increment from the process state and adds it to the nonce delta tracker.
    ///
    /// Expected stack state: [nonce_delta, ...]
    pub fn on_account_before_increment_nonce(
        &mut self,
        process: ProcessState,
    ) -> Result<(), TransactionKernelError> {
        let value = process.get_stack_item(0);
        self.account_delta.increment_nonce(value);
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
        process: ProcessState,
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
        let new_slot_value = [
            process.get_stack_item(4),
            process.get_stack_item(3),
            process.get_stack_item(2),
            process.get_stack_item(1),
        ];

        // get the current value for the slot
        let current_slot_value = [
            process.get_stack_item(8),
            process.get_stack_item(7),
            process.get_stack_item(6),
            process.get_stack_item(5),
        ];

        // update the delta tracker only if the current and new values are different
        if current_slot_value != new_slot_value {
            let slot_index = slot_index.as_int() as u8;
            self.account_delta.storage_delta().set_item(slot_index, new_slot_value);
        }

        Ok(())
    }

    /// Extracts information from the process state about the storage map being updated and
    /// records the latest values of this storage map.
    ///
    /// Expected stack state: [slot_index, NEW_MAP_KEY, NEW_MAP_VALUE, ...]
    pub fn on_account_storage_after_set_map_item(
        &mut self,
        process: ProcessState,
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
        let new_map_key = [
            process.get_stack_item(4),
            process.get_stack_item(3),
            process.get_stack_item(2),
            process.get_stack_item(1),
        ];

        // get the VALUE to which the slot is being updated
        let new_map_value = [
            process.get_stack_item(8),
            process.get_stack_item(7),
            process.get_stack_item(6),
            process.get_stack_item(5),
        ];

        let slot_index = slot_index.as_int() as u8;
        self.account_delta.storage_delta().set_map_item(
            slot_index,
            new_map_key.into(),
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
        process: ProcessState,
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
        process: ProcessState,
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

    // ADVICE INJECTOR HANDLERS
    // --------------------------------------------------------------------------------------------

    /// Returns a signature as a response to the `SigToStack` injector.
    ///
    /// This signature is created during transaction execution and stored for use as advice map
    /// inputs in the proving host. If not already present in the advice map, it is requested from
    /// the host's authenticator.
    pub fn on_signature_requested(
        &mut self,
        process: ProcessState,
        err_ctx: &ErrorContext<'_, impl MastNodeExt>,
    ) -> Result<(), TransactionKernelError> {
        let pub_key = process.get_stack_word(0);
        let msg = process.get_stack_word(1);
        let signature_key = Hasher::merge(&[pub_key.into(), msg.into()]);

        let signature = if let Some(signature) = self.adv_provider.get_mapped_values(&signature_key)
        {
            signature.to_vec()
        } else {
            let account_delta = self.account_delta.clone().into_delta();

            let signature: Vec<Felt> = match &self.authenticator {
                None => {
                    return Err(TransactionKernelError::FailedSignatureGeneration(
                        "No authenticator assigned to transaction host",
                    ));
                },
                Some(authenticator) => {
                    authenticator.get_signature(pub_key, msg, &account_delta).map_err(|_| {
                        TransactionKernelError::FailedSignatureGeneration(
                            "Error generating signature",
                        )
                    })
                },
            }?;

            self.generated_signatures.insert(signature_key, signature.clone());
            signature
        };

        for r in signature {
            self.adv_provider
                .push_stack(AdviceSource::Value(r), err_ctx)
                .map_err(|_| TransactionKernelError::FailedToPushAdviceStack(r))?;
        }

        Ok(())
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
        process: ProcessState,
        err_ctx: &ErrorContext<'_, impl MastNodeExt>,
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
            Ok(process.get_mem_word(process.ctx(), note_address)?.map(NoteId::from))
        }
    }

    /// Returns the number of storage slots initialized for the current account.
    ///
    /// # Errors
    /// Returns an error if the memory location supposed to contain the account storage slot number
    /// has not been initialized.
    fn get_num_storage_slots(process: ProcessState) -> Result<u64, TransactionKernelError> {
        let num_storage_slots_felt = process
            .get_mem_value(process.ctx(), NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR)
            .ok_or(TransactionKernelError::AccountStorageSlotsNumMissing(
                NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR,
            ))?;

        Ok(num_storage_slots_felt.as_int())
    }
}

// HOST IMPLEMENTATION FOR TRANSACTION HOST
// ================================================================================================

impl<A: AdviceProvider> Host for TransactionHost<A> {
    type AdviceProvider = A;

    fn advice_provider(&self) -> &Self::AdviceProvider {
        &self.adv_provider
    }

    fn advice_provider_mut(&mut self) -> &mut Self::AdviceProvider {
        &mut self.adv_provider
    }

    fn get_mast_forest(&self, node_digest: &Digest) -> Option<Arc<MastForest>> {
        self.mast_store.get(node_digest)
    }

    fn on_event(
        &mut self,
        process: ProcessState,
        event_id: u32,
        err_ctx: &ErrorContext<'_, impl MastNodeExt>,
    ) -> Result<(), ExecutionError> {
        let transaction_event = TransactionEvent::try_from(event_id)
            .map_err(|err| ExecutionError::event_error(Box::new(err), err_ctx))?;

        // only the `FalconSigToStack` event can be executed outside the root context
        if process.ctx() != ContextId::root()
            && !matches!(transaction_event, TransactionEvent::FalconSigToStack)
        {
            return Err(ExecutionError::event_error(
                Box::new(TransactionEventError::NotRootContext(event_id)),
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
                self.on_account_before_increment_nonce(process)
            },
            TransactionEvent::AccountAfterIncrementNonce => Ok(()),

            TransactionEvent::AccountPushProcedureIndex => {
                self.on_account_push_procedure_index(process,err_ctx)
            },

            TransactionEvent::NoteBeforeCreated => Ok(()),
            TransactionEvent::NoteAfterCreated => self.on_note_after_created(process),

            TransactionEvent::NoteBeforeAddAsset => self.on_note_before_add_asset(process),
            TransactionEvent::NoteAfterAddAsset => Ok(()),

            TransactionEvent::FalconSigToStack => self.on_signature_requested(process,err_ctx),

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
        }
        .map_err(|err| ExecutionError::event_error(Box::new(err),err_ctx))?;

        Ok(())
    }
}

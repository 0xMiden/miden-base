use alloc::vec::Vec;

use miden_lib::transaction::memory::{
    ACCOUNT_STACK_TOP_PTR,
    ACTIVE_INPUT_NOTE_PTR,
    NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR,
};
use miden_objects::Word;
use miden_objects::account::AccountId;
use miden_objects::note::{NoteId, NoteInputs};
use miden_processor::{EventError, ExecutionError, Felt, ProcessState};

use crate::errors::TransactionKernelError;

// TRANSACTION KERNEL PROCESS
// ================================================================================================

pub(super) trait TransactionKernelProcess {
    fn get_active_account_id(&self) -> Result<AccountId, TransactionKernelError>;

    fn get_num_storage_slots(&self) -> Result<u64, TransactionKernelError>;

    fn get_vault_root(&self, vault_root_ptr: Felt) -> Result<Word, TransactionKernelError>;

    fn get_active_note_id(&self) -> Result<Option<NoteId>, EventError>;

    fn read_note_recipient_info_from_adv_map(
        &self,
        recipient_digest: Word,
    ) -> Result<(NoteInputs, Word, Word), TransactionKernelError>;

    fn read_note_inputs_from_adv_map(
        &self,
        inputs_commitment: &Word,
        num_inputs: usize,
    ) -> Result<NoteInputs, TransactionKernelError>;

    fn has_advice_map_entry(&self, key: Word) -> bool;
}

impl<'a> TransactionKernelProcess for ProcessState<'a> {
    /// Returns the ID of the currently active account.
    fn get_active_account_id(&self) -> Result<AccountId, TransactionKernelError> {
        let account_stack_top_ptr =
            self.get_mem_value(self.ctx(), ACCOUNT_STACK_TOP_PTR).ok_or_else(|| {
                TransactionKernelError::other("account stack top ptr should be initialized")
            })?;
        let account_stack_top_ptr = u32::try_from(account_stack_top_ptr).map_err(|_| {
            TransactionKernelError::other("account stack top ptr should fit into a u32")
        })?;

        let active_account_ptr = self
            .get_mem_value(self.ctx(), account_stack_top_ptr)
            .ok_or_else(|| TransactionKernelError::other("account id should be initialized"))?;
        let active_account_ptr = u32::try_from(active_account_ptr).map_err(|_| {
            TransactionKernelError::other("active account ptr should fit into a u32")
        })?;

        let active_account_id_and_nonce = self
            .get_mem_word(self.ctx(), active_account_ptr)
            .map_err(|_| {
                TransactionKernelError::other("active account ptr should be word-aligned")
            })?
            .ok_or_else(|| {
                TransactionKernelError::other("active account id should be initialized")
            })?;

        AccountId::try_from([active_account_id_and_nonce[1], active_account_id_and_nonce[0]])
            .map_err(|_| {
                TransactionKernelError::other(
                    "active account id ptr should point to a valid account ID",
                )
            })
    }

    /// Returns the number of storage slots initialized for the active account.
    ///
    /// # Errors
    /// Returns an error if the memory location supposed to contain the account storage slot number
    /// has not been initialized.
    fn get_num_storage_slots(&self) -> Result<u64, TransactionKernelError> {
        let num_storage_slots_felt = self
            .get_mem_value(self.ctx(), NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR)
            .ok_or(TransactionKernelError::AccountStorageSlotsNumMissing(
                NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR,
            ))?;

        Ok(num_storage_slots_felt.as_int())
    }

    /// Returns the ID of the active note, or None if the note execution hasn't started yet or has
    /// already ended.
    ///
    /// # Errors
    /// Returns an error if the address of the active note is invalid (e.g., greater than
    /// `u32::MAX`).
    fn get_active_note_id(&self) -> Result<Option<NoteId>, EventError> {
        // get the note address in `Felt` or return `None` if the address hasn't been accessed
        // previously.
        let note_address_felt = match self.get_mem_value(self.ctx(), ACTIVE_INPUT_NOTE_PTR) {
            Some(addr) => addr,
            None => return Ok(None),
        };
        // convert note address into u32
        let note_address = u32::try_from(note_address_felt).map_err(|_| {
            EventError::from(format!(
                "failed to convert {note_address_felt} into a memory address (u32)"
            ))
        })?;
        // if `note_address` == 0 note execution has ended and there is no valid note address
        if note_address == 0 {
            Ok(None)
        } else {
            Ok(self
                .get_mem_word(self.ctx(), note_address)
                .map_err(ExecutionError::MemoryError)?
                .map(NoteId::from))
        }
    }

    /// Returns the vault root at the provided pointer.
    fn get_vault_root(&self, vault_root_ptr: Felt) -> Result<Word, TransactionKernelError> {
        let vault_root_ptr = u32::try_from(vault_root_ptr).map_err(|_err| {
            TransactionKernelError::other(format!(
                "vault root ptr should fit into a u32, but was {vault_root_ptr}"
            ))
        })?;
        self.get_mem_word(self.ctx(), vault_root_ptr)
            .map_err(|_err| {
                TransactionKernelError::other(format!(
                    "vault root ptr {vault_root_ptr} is not word-aligned"
                ))
            })?
            .ok_or_else(|| {
                TransactionKernelError::other(format!(
                    "vault root ptr {vault_root_ptr} was not initialized"
                ))
            })
    }

    fn read_note_recipient_info_from_adv_map(
        &self,
        recipient_digest: Word,
    ) -> Result<(NoteInputs, Word, Word), TransactionKernelError> {
        let recipient_data = self
            .advice_provider()
            .get_mapped_values(&recipient_digest)
            .ok_or_else(|| TransactionKernelError::MalformedRecipientData(Vec::new()))?;

        if recipient_data.len() != 13 {
            return Err(TransactionKernelError::MalformedRecipientData(recipient_data.to_vec()));
        }

        let num_inputs = recipient_data[0].as_int() as usize;
        let inputs_commitment =
            Word::new([recipient_data[1], recipient_data[2], recipient_data[3], recipient_data[4]]);
        let script_root =
            Word::new([recipient_data[5], recipient_data[6], recipient_data[7], recipient_data[8]]);
        let serial_num = Word::new([
            recipient_data[9],
            recipient_data[10],
            recipient_data[11],
            recipient_data[12],
        ]);

        let inputs = self.read_note_inputs_from_adv_map(&inputs_commitment, num_inputs)?;

        if inputs.commitment() != inputs_commitment {
            return Err(TransactionKernelError::InvalidNoteInputs {
                expected: inputs_commitment,
                actual: inputs.commitment(),
            });
        }

        Ok((inputs, script_root, serial_num))
    }

    /// Extracts and validates note inputs from the advice provider using the stored input length.
    fn read_note_inputs_from_adv_map(
        &self,
        inputs_commitment: &Word,
        num_inputs: usize,
    ) -> Result<NoteInputs, TransactionKernelError> {
        match self.advice_provider().get_mapped_values(inputs_commitment) {
            None => Ok(NoteInputs::default()),
            Some(inputs) => {
                if num_inputs > inputs.len() {
                    return Err(TransactionKernelError::TooFewElementsForNoteInputs {
                        specified: num_inputs as u64,
                        actual: inputs.len() as u64,
                    });
                }

                let values = inputs[0..num_inputs].to_vec();

                NoteInputs::new(values).map_err(TransactionKernelError::MalformedNoteInputs)
            },
        }
    }

    fn has_advice_map_entry(&self, key: Word) -> bool {
        self.advice_provider().get_mapped_values(&key).is_some()
    }
}

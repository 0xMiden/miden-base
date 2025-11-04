use miden_lib::transaction::memory::{
    ACCOUNT_STACK_TOP_PTR,
    ACTIVE_INPUT_NOTE_PTR,
    NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR,
};
use miden_objects::account::AccountId;
use miden_objects::note::{NoteId, NoteStorage};
use miden_objects::{Word, ZERO};
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
    ) -> Result<(NoteStorage, Word, Word), TransactionKernelError>;

    fn read_note_inputs_from_adv_map(
        &self,
        inputs_commitment: &Word,
    ) -> Result<NoteStorage, TransactionKernelError>;

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
    ) -> Result<(NoteStorage, Word, Word), TransactionKernelError> {
        let (sn_script_hash, storage_commitment) =
            read_double_word_from_adv_map(self, recipient_digest)?;
        let (sn_hash, script_root) = read_double_word_from_adv_map(self, sn_script_hash)?;
        let (serial_num, _) = read_double_word_from_adv_map(self, sn_hash)?;

        let note_storage = self.read_note_inputs_from_adv_map(&storage_commitment)?;

        Ok((note_storage, script_root, serial_num))
    }

    /// Extracts and validates note inputs from the advice provider using trial unhashing.
    ///
    /// This function tries to determine the correct number of inputs by:
    /// 1. Finding the last non-zero element as a starting point
    /// 2. Building NoteStorage and checking if the hash matches inputs_commitment
    /// 3. If not, incrementing num_inputs and trying again (up to 6 more times)
    /// 4. If num_inputs grows to the size of inputs_data and there's still no match, returning an
    ///    error
    fn read_note_inputs_from_adv_map(
        &self,
        storage_commitment: &Word,
    ) -> Result<NoteStorage, TransactionKernelError> {
        let storage_data = self.advice_provider().get_mapped_values(storage_commitment);

        let storage = match storage_data {
            None => NoteStorage::default(),
            Some(inputs) => {
                // Start with the last non-zero element as a hint
                let initial_num_inputs =
                    inputs.iter().rposition(|&x| x != ZERO).map(|pos| pos + 1).unwrap_or(0);

                // Try different input counts using trial unhashing
                let mut num_inputs = initial_num_inputs;

                loop {
                    let candidate_storage_item =
                        NoteStorage::new(inputs[0..num_inputs].to_vec())
                            .map_err(TransactionKernelError::MalformedNoteStorage)?;

                    if candidate_storage_item.commitment() == *storage_commitment {
                        return Ok(candidate_storage_item);
                    }

                    num_inputs += 1;
                    if num_inputs > inputs.len() {
                        break;
                    }
                }

                // If we've exhausted all attempts, return an error
                return Err(TransactionKernelError::InvalidNoteStorage {
                    expected: *storage_commitment,
                    actual: NoteStorage::new(inputs[0..num_inputs.min(inputs.len())].to_vec())
                        .map(|i| i.commitment())
                        .unwrap_or_default(),
                });
            },
        };

        Ok(storage)
    }

    fn has_advice_map_entry(&self, key: Word) -> bool {
        self.advice_provider().get_mapped_values(&key).is_some()
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Reads a double word (two [`Word`]s, 8 [`Felt`]s total) from the advice map.
///
/// # Errors
/// Returns an error if the key is not present in the advice map or if the data is malformed
/// (not exactly 8 elements).
fn read_double_word_from_adv_map(
    process: &ProcessState,
    key: Word,
) -> Result<(Word, Word), TransactionKernelError> {
    let data = process
        .advice_provider()
        .get_mapped_values(&key)
        .ok_or_else(|| TransactionKernelError::MalformedRecipientData(vec![]))?;

    if data.len() != 8 {
        return Err(TransactionKernelError::MalformedRecipientData(data.to_vec()));
    }

    let first_word = Word::new([data[0], data[1], data[2], data[3]]);
    let second_word = Word::new([data[4], data[5], data[6], data[7]]);

    Ok((first_word, second_word))
}

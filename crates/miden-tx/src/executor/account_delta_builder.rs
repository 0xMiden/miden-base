use alloc::collections::BTreeMap;

use miden_lib::transaction::memory::{
    ACCOUNT_DELTA_INITIAL_STORAGE_SLOTS, ACCOUNT_DELTA_NONCE_PTR, ACCT_STORAGE_SLOT_NUM_ELEMENTS,
    NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR, NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR,
};
use miden_objects::{
    Felt, Word,
    account::{AccountDelta, AccountStorageDelta, AccountVaultDelta, StorageSlotType},
};
use vm_processor::{ContextId, Process, ProcessState};

/// Builds an [`AccountDelta`] from a process after the transaction kernel was executed.
///
/// Assumes the tx kernel was executed and relies on certain memory being initialized.
pub struct AccountDeltaBuilder<'process> {
    state: ProcessState<'process>,
}

impl<'process> AccountDeltaBuilder<'process> {
    /// Creates a new account delta builder from the provided [`Process`].
    pub fn new(process: &'process Process) -> Self {
        Self { state: process.into() }
    }

    /// Returns the element at the specified address in kernel memory.
    fn get_mem_value(&self, addr: u32) -> Option<Felt> {
        self.state.get_mem_value(ContextId::root(), addr)
    }

    /// Returns the batch of elements starting at the specified address in kernel memory.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - the provided address is not word-aligned.
    fn get_mem_word(&self, addr: u32) -> Option<Word> {
        self.state
            .get_mem_word(ContextId::root(), addr)
            .expect("address should be aligned")
    }

    /// Builds the account delta from the process' memory.
    pub fn build(self) -> AccountDelta {
        let nonce = self.get_mem_value(ACCOUNT_DELTA_NONCE_PTR);

        let storage_delta = self.build_storage_delta();
        let vault_delta = AccountVaultDelta::default();

        // TODO: Add the account ID to the delta struct so it is self-contained and its
        // commitment can be computed without having to provide the account ID.
        AccountDelta::new(storage_delta, vault_delta, nonce)
            .expect("kernel should ensure nonce is incremented if state has changed")
    }

    fn build_storage_delta(&self) -> AccountStorageDelta {
        let num_storage_slots = self.num_storage_slots();
        let mut value_slots = BTreeMap::new();
        let map_slots = BTreeMap::new();

        for slot_idx in 0..num_storage_slots {
            let (slot_type, slot_data) = self.get_account_slot(slot_idx);

            match slot_type {
                StorageSlotType::Value => {
                    // Compute diff between initial and current value.
                    let initial_value = self.get_initial_account_item(slot_idx);
                    if slot_data != initial_value {
                        value_slots.insert(slot_idx, slot_data);
                    }
                },
                // TODO: Maps.
                StorageSlotType::Map => (),
            }
        }

        // SAFETY: We iterate over the slot indices once, so every index is only inserted into one
        // map.
        AccountStorageDelta::new(value_slots, map_slots)
            .expect("we should not have inserted the same index twice")
    }

    fn num_storage_slots(&self) -> u8 {
        let num_storage_slots_felt = self
            .get_mem_value(NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR)
            .expect("number of native account storage slots should be initialized");

        u8::try_from(num_storage_slots_felt.as_int()).expect("storage slot num should fit into u8")
    }

    fn get_account_slot(&self, slot_idx: u8) -> (StorageSlotType, Word) {
        // Each slot has a size of 8 elements.
        let slot_offset = (slot_idx * ACCT_STORAGE_SLOT_NUM_ELEMENTS) as u32;
        let slot_data = self
            .get_mem_word(NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR + slot_offset)
            .expect("native account storage slots should be initialized");
        let slot_metadata = self
            .get_mem_word(NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR + slot_offset + 4)
            .expect("native account storage slots should be initialized");
        let slot_type = StorageSlotType::try_from(slot_metadata[0])
            .expect("slot type in kernel memory should be valid");
        (slot_type, slot_data)
    }

    fn get_initial_account_item(&self, slot_idx: u8) -> Word {
        // Each slot has a size of 8 elements.
        let slot_offset = (slot_idx * ACCT_STORAGE_SLOT_NUM_ELEMENTS) as u32;
        self.get_mem_word(ACCOUNT_DELTA_INITIAL_STORAGE_SLOTS + slot_offset)
            .expect("initial account storage slots should be initialized")
    }
}

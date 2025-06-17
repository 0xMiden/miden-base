use alloc::collections::BTreeMap;

use miden_lib::transaction::memory::{
    ACCOUNT_DELTA_FUNGIBLE_ASSET_PTR, ACCOUNT_DELTA_INITIAL_STORAGE_SLOTS, ACCOUNT_DELTA_NONCE_PTR,
    ACCT_STORAGE_SLOT_NUM_ELEMENTS, NATIVE_ACCT_STORAGE_SLOTS_SECTION_PTR,
    NATIVE_NUM_ACCT_STORAGE_SLOTS_PTR,
};
use miden_objects::{
    Felt, Word,
    account::{
        AccountDelta, AccountId, AccountStorageDelta, AccountVaultDelta, FungibleAssetDelta,
        NonFungibleAssetDelta, StorageSlotType,
    },
    asset::FungibleAsset,
};
use vm_processor::{ContextId, Process, ProcessState};

use crate::host::LinkMap;

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
        let vault_delta = self.build_vault_delta();

        // TODO: Add the account ID to the delta struct so it is self-contained and its
        // commitment can be computed without having to provide the account ID.
        AccountDelta::new(storage_delta, vault_delta, nonce)
            .expect("kernel should ensure nonce is incremented if state has changed")
    }

    fn build_vault_delta(&self) -> AccountVaultDelta {
        let fungible = self.build_fungible_vault_delta();
        let non_fungible = NonFungibleAssetDelta::default();
        AccountVaultDelta::new(fungible, non_fungible)
    }

    fn build_fungible_vault_delta(&self) -> FungibleAssetDelta {
        let delta_map = LinkMap::new(Felt::from(ACCOUNT_DELTA_FUNGIBLE_ASSET_PTR), self.state);
        let mut delta =
            FungibleAssetDelta::new(BTreeMap::new()).expect("empty delta should be valid");

        for asset_delta in delta_map.iter() {
            let faucet_id = AccountId::try_from([asset_delta.key[3], asset_delta.key[2]])
                .expect("TODO: tx kernel does not guarantee faucet ID validity");
            let amount_hi: u32 = asset_delta.value0[3]
                .try_into()
                .expect("tx kernel should guarantee amount limbs are u32");
            let amount_lo: u32 = asset_delta.value0[2]
                .try_into()
                .expect("tx kernel should guarantee amount limbs are u32");
            let amount: u64 = ((amount_hi as u64) << 32) + amount_lo as u64;
            let signed_amount: i64 = amount
                .try_into()
                .expect("tx kernel should guarantee that the delta is in i64 range");
            if amount > 0 {
                let asset = FungibleAsset::new(faucet_id, amount)
                    .expect("TODO: faucet ID should be valid?");
                // SAFETY: The tx kernel guarantees there is one vault delta entry per fungible
                // asset so the removed amount will not overflow the total delta amount.
                delta.add(asset).expect("adding an i64 to 0 should not overflow an i64");
            } else {
                let asset = FungibleAsset::new(faucet_id, signed_amount.unsigned_abs())
                    .expect("TODO: faucet ID should be valid?");
                // SAFETY: The tx kernel guarantees there is one vault delta entry per fungible
                // asset so the removed amount will not overflow the total delta amount.
                delta.remove(asset).expect("removing an i64 from 0 should not underflow an i64");
            }
        }

        delta
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

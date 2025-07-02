use miden_objects::{
    Felt, ZERO,
    account::{
        AccountDelta, AccountId, AccountStorageDelta, AccountStorageHeader, AccountVaultDelta,
    },
};

use crate::host::account_init_storage::AccountInitialStorage;
// ACCOUNT DELTA TRACKER
// ================================================================================================

/// Keeps track of changes made to the account during transaction execution.
///
/// Currently, this tracks:
/// - Changes to the account storage, slots and maps.
/// - Changes to the account vault.
/// - Changes to the account nonce.
///
/// TODO: implement tracking of:
/// - account code changes.
#[derive(Debug, Clone)]
pub struct AccountDeltaTracker {
    account_id: AccountId,
    storage: AccountStorageDelta,
    init_storage: AccountInitialStorage,
    vault: AccountVaultDelta,
    nonce_increment: Felt,
}

impl AccountDeltaTracker {
    /// Returns a new [AccountDeltaTracker] instantiated for the specified account.
    pub fn new(account_id: AccountId, storage_header: AccountStorageHeader) -> Self {
        Self {
            account_id,
            storage: AccountStorageDelta::new(),
            init_storage: AccountInitialStorage::new(storage_header),
            vault: AccountVaultDelta::default(),
            nonce_increment: ZERO,
        }
    }

    /// Tracks nonce delta.
    pub fn increment_nonce(&mut self, value: Felt) {
        self.nonce_increment += value;
    }

    /// Get a mutable reference to the current vault delta
    pub fn vault_delta(&mut self) -> &mut AccountVaultDelta {
        &mut self.vault
    }

    /// Get a mutable reference to the initial storage.
    pub fn init_storage(&mut self) -> &mut AccountInitialStorage {
        &mut self.init_storage
    }

    /// Get a mutable reference to the current storage delta
    pub fn storage_delta(&mut self) -> &mut AccountStorageDelta {
        &mut self.storage
    }

    /// Consumes `self` and returns the resulting [AccountDelta].
    pub fn into_delta(self) -> AccountDelta {
        let account_id = self.account_id;
        let nonce_increment = self.nonce_increment;

        let (vault_delta, storage_delta) = self.normalize();

        AccountDelta::new(account_id, storage_delta, vault_delta, nonce_increment)
            .expect("account delta created in delta tracker should be valid")
    }

    /// Normalizes the storage delta by:
    /// - removing entries for value slot updates whose new value is equal to the initial value at
    ///   the beginning of transaction execution.
    /// - removing entries for map slot updates where for a given key, the new value is equal to the
    ///   initial value at the beginning of transaction execution.
    fn normalize(self) -> (AccountVaultDelta, AccountStorageDelta) {
        let (mut value_slots, mut map_slots) = self.storage.into_parts();
        let vault_delta = self.vault;

        // Keep only the values whose new value is different from the initial value.
        value_slots.retain(|slot_idx, new_value| {
            // SAFETY: The header in the intial storage is the one from the account against which
            // the transaction is executed, so accessing that slot index should be fine.
            let (_, initial_value) = self
                .init_storage
                .storage_header()
                .slot(*slot_idx as usize)
                .expect("index should be in bounds");
            new_value != initial_value
        });

        // Keep only the map values whose new value is different from the initial value.
        map_slots.iter_mut().for_each(|(slot_idx, map_delta)| {
            if let Some(init_map) = self.init_storage.init_map(*slot_idx) {
                map_delta.as_map_mut().retain(|key, value| match init_map.get(key.inner()) {
                    Some(init_value) => value != init_value,
                    None => true,
                });
            }
        });

        let storage_delta = AccountStorageDelta::from_parts(value_slots, map_slots)
            .expect("storage delta should still be valid since no new values were added");

        (vault_delta, storage_delta)
    }
}

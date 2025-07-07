use miden_objects::{
    Felt, ZERO,
    account::{AccountDelta, AccountId, AccountStorageHeader, AccountVaultDelta},
};

use crate::host::storage_delta_tracker::StorageDeltaTracker;

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
    storage: StorageDeltaTracker,
    vault: AccountVaultDelta,
    nonce_delta: Felt,
}

impl AccountDeltaTracker {
    /// Returns a new [AccountDeltaTracker] instantiated for the specified account.
    pub fn new(account_id: AccountId, storage_header: AccountStorageHeader) -> Self {
        Self {
            account_id,
            storage: StorageDeltaTracker::new(storage_header),
            vault: AccountVaultDelta::default(),
            nonce_delta: ZERO,
        }
    }

    /// Tracks nonce delta.
    pub fn increment_nonce(&mut self, value: Felt) {
        self.nonce_delta += value;
    }

    /// Get a mutable reference to the current vault delta
    pub fn vault_delta(&mut self) -> &mut AccountVaultDelta {
        &mut self.vault
    }

    /// Returns a mutable reference to the current storage delta tracker.
    pub fn storage(&mut self) -> &mut StorageDeltaTracker {
        &mut self.storage
    }

    /// Consumes `self` and returns the resulting [AccountDelta].
    ///
    /// Normalizes the delta by removing entries for storage slots where the initial and new
    /// value are equal.
    pub fn into_delta(self) -> AccountDelta {
        let account_id = self.account_id;
        let nonce_delta = self.nonce_delta;

        let storage_delta = self.storage.into_delta();
        let vault_delta = self.vault;

        AccountDelta::new(account_id, storage_delta, vault_delta, nonce_delta)
            .expect("account delta created in delta tracker should be valid")
    }
}

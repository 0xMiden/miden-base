use miden_objects::{
    Felt, ZERO,
    account::{
        AccountDelta, AccountId, AccountStorageDelta, AccountStorageHeader, AccountVaultDelta,
    },
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
    storage_delta_tracker: StorageDeltaTracker,
    vault: AccountVaultDelta,
    nonce_increment: Felt,
}

impl AccountDeltaTracker {
    /// Returns a new [AccountDeltaTracker] instantiated for the specified account.
    pub fn new(account_id: AccountId, storage_header: AccountStorageHeader) -> Self {
        Self {
            account_id,
            storage_delta_tracker: StorageDeltaTracker::new(storage_header),
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

    /// Returns a mutable reference to the current storage delta tracker.
    pub fn storage_delta_tracker(&mut self) -> &mut StorageDeltaTracker {
        &mut self.storage_delta_tracker
    }

    /// Consumes `self` and returns the resulting [AccountDelta].
    pub fn into_delta(self) -> AccountDelta {
        let account_id = self.account_id;
        let nonce_increment = self.nonce_increment;

        let (vault_delta, storage_delta) = self.normalize();

        AccountDelta::new(account_id, storage_delta, vault_delta, nonce_increment)
            .expect("account delta created in delta tracker should be valid")
    }

    /// Normalizes the delta by removing entries for storage slots where the initial and new value
    /// are equal.
    fn normalize(self) -> (AccountVaultDelta, AccountStorageDelta) {
        let storage_delta = self.storage_delta_tracker.into_delta();
        let vault_delta = self.vault;

        (vault_delta, storage_delta)
    }
}

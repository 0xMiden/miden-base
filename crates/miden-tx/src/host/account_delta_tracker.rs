use miden_objects::{
    Felt, ZERO,
    account::{
        AccountDelta, AccountId, AccountStorageDelta, AccountStorageHeader, AccountVaultDelta,
    },
};

use crate::host::account_storage_tracker::AccountInitStorageTracker;
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
    init_storage: AccountInitStorageTracker,
    vault: AccountVaultDelta,
    nonce_increment: Felt,
}

impl AccountDeltaTracker {
    /// Returns a new [AccountDeltaTracker] instantiated for the specified account.
    pub fn new(account_id: AccountId, storage_header: AccountStorageHeader) -> Self {
        Self {
            account_id,
            storage: AccountStorageDelta::new(),
            init_storage: AccountInitStorageTracker::new(storage_header),
            vault: AccountVaultDelta::default(),
            nonce_increment: ZERO,
        }
    }

    /// Consumes `self` and returns the resulting [AccountDelta].
    pub fn into_delta(mut self) -> AccountDelta {
        self.storage.normalize(self.init_storage.storage_header());

        AccountDelta::new(self.account_id, self.storage, self.vault, self.nonce_increment)
            .expect("account delta created in delta tracker should be valid")
    }

    /// Tracks nonce delta.
    pub fn increment_nonce(&mut self, value: Felt) {
        self.nonce_increment += value;
    }

    /// Get a mutable reference to the current vault delta
    pub fn vault_delta(&mut self) -> &mut AccountVaultDelta {
        &mut self.vault
    }

    /// Get a mutable reference to the current storage delta
    pub fn storage_delta(&mut self) -> &mut AccountStorageDelta {
        &mut self.storage
    }
}

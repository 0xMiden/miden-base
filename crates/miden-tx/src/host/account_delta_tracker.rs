use miden_objects::{
    Felt, ZERO,
    account::{AccountDelta, AccountId, AccountStorageDelta, AccountVaultDelta},
};
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
/// - all account storage changes.
/// - account code changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountDeltaTracker {
    account_id: AccountId,
    storage: AccountStorageDelta,
    vault: AccountVaultDelta,
    init_nonce: Felt,
    nonce_delta: Felt,
}

impl AccountDeltaTracker {
    /// Returns a new [AccountDeltaTracker] instantiated for the specified account.
    pub fn new(account_id: AccountId) -> Self {
        Self {
            account_id,
            storage: AccountStorageDelta::default(),
            vault: AccountVaultDelta::default(),
            init_nonce: account.nonce(),
            nonce_delta: ZERO,
        }
    }

    /// Consumes `self` and returns the resulting [AccountDelta].
    pub fn into_delta(self) -> AccountDelta {
        let nonce_delta = (self.nonce_delta != ZERO).then_some(self.init_nonce + self.nonce_delta);

        AccountDelta::new(self.account_id, self.storage, self.vault, nonce_delta)
            .expect("account delta created in delta tracker should be valid")
    }

    /// Tracks nonce delta.
    pub fn increment_nonce(&mut self, value: Felt) {
        self.nonce_delta += value;
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

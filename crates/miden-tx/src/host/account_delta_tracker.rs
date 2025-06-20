use miden_objects::{
    Felt, ZERO,
    account::{AccountDelta, AccountStorageDelta, AccountVaultDelta},
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
    storage: AccountStorageDelta,
    vault: AccountVaultDelta,
    nonce_delta: Felt,
}

impl AccountDeltaTracker {
    /// Returns a new [AccountDeltaTracker] instantiated for the specified account.
    pub fn new() -> Self {
        Self {
            storage: AccountStorageDelta::default(),
            vault: AccountVaultDelta::default(),
            nonce_delta: ZERO,
        }
    }

    /// Consumes `self` and returns the resulting [AccountDelta].
    pub fn into_delta(self) -> AccountDelta {
        let nonce_delta = if self.nonce_delta != ZERO {
            Some(self.nonce_delta)
        } else {
            None
        };
        AccountDelta::new(self.storage, self.vault, nonce_delta).expect("invalid account delta")
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

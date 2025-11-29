use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use miden_objects::account::auth::PublicKeyCommitment;
use miden_objects::account::{AccountComponent, StorageMap, StorageMapKey, StorageSlot};
use miden_objects::{AccountError, Word};

use crate::account::components::ecdsa_k256_keccak_multisig_library;

// MULTISIG AUTHENTICATION COMPONENT
// ================================================================================================

/// Configuration for [`AuthEcdsaK256KeccakMultisig`] component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthEcdsaK256KeccakMultisigConfig {
    approvers: Vec<PublicKeyCommitment>,
    default_threshold: u32,
    proc_thresholds: Vec<(Word, u32)>,
}

impl AuthEcdsaK256KeccakMultisigConfig {
    /// Creates a new configuration with the given approvers and a default threshold.
    ///
    /// The `default_threshold` must be at least 1 and at most the number of approvers.
    pub fn new(
        approvers: Vec<PublicKeyCommitment>,
        default_threshold: u32,
    ) -> Result<Self, AccountError> {
        if default_threshold == 0 {
            return Err(AccountError::other("threshold must be at least 1"));
        }
        if default_threshold > approvers.len() as u32 {
            return Err(AccountError::other(
                "threshold cannot be greater than number of approvers",
            ));
        }

        // Check for duplicate approvers
        if approvers.len() != approvers.iter().collect::<BTreeSet<_>>().len() {
            return Err(AccountError::other("duplicate approver public keys are not allowed"));
        }

        Ok(Self {
            approvers,
            default_threshold,
            proc_thresholds: vec![],
        })
    }

    /// Attaches a per-procedure threshold map. Each procedure threshold must be at least 1 and
    /// at most the number of approvers.
    pub fn with_proc_thresholds(
        mut self,
        proc_thresholds: Vec<(Word, u32)>,
    ) -> Result<Self, AccountError> {
        for (_, threshold) in &proc_thresholds {
            if *threshold == 0 {
                return Err(AccountError::other("procedure threshold must be at least 1"));
            }
            if *threshold > self.approvers.len() as u32 {
                return Err(AccountError::other(
                    "procedure threshold cannot be greater than number of approvers",
                ));
            }
        }
        self.proc_thresholds = proc_thresholds;
        Ok(self)
    }

    pub fn approvers(&self) -> &[PublicKeyCommitment] {
        &self.approvers
    }

    pub fn default_threshold(&self) -> u32 {
        self.default_threshold
    }

    pub fn proc_thresholds(&self) -> &[(Word, u32)] {
        &self.proc_thresholds
    }
}

/// An [`AccountComponent`] implementing a multisig based on ECDSA signatures.
///
/// It enforces a threshold of approver signatures for every transaction, with optional
/// per-procedure thresholds overrides. Non-uniform thresholds (especially a threshold of one)
/// should be used with caution for private multisig accounts, as a single approver could withhold
///  the new state from other approvers, effectively locking them out.
///
/// The storage layout is:
/// - Slot 0(value): [threshold, num_approvers, 0, 0]
/// - Slot 1(map): A map with approver public keys (index -> pubkey)
/// - Slot 2(map): A map which stores executed transactions
/// - Slot 3(map): A map which stores procedure thresholds (PROC_ROOT -> threshold)
///
/// This component supports all account types.
#[derive(Debug)]
pub struct AuthEcdsaK256KeccakMultisig {
    config: AuthEcdsaK256KeccakMultisigConfig,
}

impl AuthEcdsaK256KeccakMultisig {
    /// Creates a new [`AuthEcdsaK256KeccakMultisig`] component from the provided configuration.
    pub fn new(config: AuthEcdsaK256KeccakMultisigConfig) -> Result<Self, AccountError> {
        Ok(Self { config })
    }
}

impl From<AuthEcdsaK256KeccakMultisig> for AccountComponent {
    fn from(multisig: AuthEcdsaK256KeccakMultisig) -> Self {
        let mut storage_slots = Vec::with_capacity(3);

        // Slot 0: [threshold, num_approvers, 0, 0]
        let num_approvers = multisig.config.approvers().len() as u32;
        storage_slots.push(StorageSlot::Value(Word::from([
            multisig.config.default_threshold(),
            num_approvers,
            0,
            0,
        ])));

        // Slot 1: A map with approver public keys
        let map_entries = multisig
            .config
            .approvers()
            .iter()
            .enumerate()
            .map(|(i, pub_key)| (Word::from([i as u32, 0, 0, 0]).into(), (*pub_key).into()));

        // Safe to unwrap because we know that the map keys are unique.
        storage_slots.push(StorageSlot::Map(StorageMap::with_entries(map_entries).unwrap()));

        // Slot 2: A map which stores executed transactions
        let executed_transactions = StorageMap::default();
        storage_slots.push(StorageSlot::Map(executed_transactions));

        // Slot 3: A map which stores procedure thresholds (PROC_ROOT -> threshold)
        let proc_threshold_roots = StorageMap::with_entries(
            multisig.config.proc_thresholds().iter().map(|(proc_root, threshold)| {
                (StorageMapKey::from(*proc_root), Word::from([*threshold, 0, 0, 0]))
            }),
        )
        .unwrap();
        storage_slots.push(StorageSlot::Map(proc_threshold_roots));

        AccountComponent::new(ecdsa_k256_keccak_multisig_library(), storage_slots)
            .expect("Multisig auth component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use miden_objects::Word;
    use miden_objects::account::AccountBuilder;

    use super::*;
    use crate::account::wallets::BasicWallet;

    /// Test multisig component setup with various configurations
    #[test]
    fn test_multisig_component_setup() {
        // Create test public keys
        let pub_key_1 = PublicKeyCommitment::from(Word::from([1u32, 0, 0, 0]));
        let pub_key_2 = PublicKeyCommitment::from(Word::from([2u32, 0, 0, 0]));
        let pub_key_3 = PublicKeyCommitment::from(Word::from([3u32, 0, 0, 0]));
        let approvers = vec![pub_key_1, pub_key_2, pub_key_3];
        let threshold = 2u32;

        // Create multisig component
        let multisig_component = AuthEcdsaK256KeccakMultisig::new(
            AuthEcdsaK256KeccakMultisigConfig::new(approvers.clone(), threshold)
                .expect("invalid multisig config"),
        )
        .expect("multisig component creation failed");

        // Build account with multisig component
        let account = AccountBuilder::new([0; 32])
            .with_auth_component(multisig_component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        // Verify slot 0: [threshold, num_approvers, 0, 0]
        let threshold_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(threshold_slot, Word::from([threshold, approvers.len() as u32, 0, 0]));

        // Verify slot 1: Approver public keys in map
        for (i, expected_pub_key) in approvers.iter().enumerate() {
            let stored_pub_key = account
                .storage()
                .get_map_item(1, Word::from([i as u32, 0, 0, 0]).into())
                .expect("storage map access failed");
            assert_eq!(stored_pub_key, Word::from(*expected_pub_key));
        }
    }

    /// Test multisig component with minimum threshold (1 of 1)
    #[test]
    fn test_multisig_component_minimum_threshold() {
        let pub_key = PublicKeyCommitment::from(Word::from([42u32, 0, 0, 0]));
        let approvers = vec![pub_key];
        let threshold = 1u32;

        let multisig_component = AuthEcdsaK256KeccakMultisig::new(
            AuthEcdsaK256KeccakMultisigConfig::new(approvers.clone(), threshold)
                .expect("invalid multisig config"),
        )
        .expect("multisig component creation failed");

        let account = AccountBuilder::new([0; 32])
            .with_auth_component(multisig_component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        // Verify storage layout
        let threshold_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(threshold_slot, Word::from([threshold, approvers.len() as u32, 0, 0]));

        let stored_pub_key = account
            .storage()
            .get_map_item(1, Word::from([0u32, 0, 0, 0]).into())
            .expect("storage map access failed");
        assert_eq!(stored_pub_key, Word::from(pub_key));
    }

    /// Test multisig component error cases
    #[test]
    fn test_multisig_component_error_cases() {
        let pub_key = PublicKeyCommitment::from(Word::from([1u32, 0, 0, 0]));
        let approvers = vec![pub_key];

        // Test threshold = 0 (should fail)
        let result = AuthEcdsaK256KeccakMultisigConfig::new(approvers.clone(), 0);
        assert!(result.unwrap_err().to_string().contains("threshold must be at least 1"));

        // Test threshold > number of approvers (should fail)
        let result = AuthEcdsaK256KeccakMultisigConfig::new(approvers, 2);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("threshold cannot be greater than number of approvers")
        );
    }

    /// Test multisig component with duplicate approvers (should fail)
    #[test]
    fn test_multisig_component_duplicate_approvers() {
        let pub_key_1 = PublicKeyCommitment::from(Word::from([1u32, 0, 0, 0]));
        let pub_key_2 = PublicKeyCommitment::from(Word::from([2u32, 0, 0, 0]));

        // Test with duplicate approvers (should fail)
        let approvers = vec![pub_key_1, pub_key_2, pub_key_1];
        let result = AuthEcdsaK256KeccakMultisigConfig::new(approvers, 2);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("duplicate approver public keys are not allowed")
        );
    }
}

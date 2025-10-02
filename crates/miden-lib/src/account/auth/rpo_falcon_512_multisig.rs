use alloc::vec::Vec;

use miden_objects::account::{AccountComponent, StorageMap, StorageSlot};
use miden_objects::{AccountError, Word};

use crate::account::auth::PublicKeyCommitment;
use crate::account::components::rpo_falcon_512_multisig_library;

// MULTISIG AUTHENTICATION COMPONENT
// ================================================================================================

/// Configuration for [`AuthRpoFalcon512MultisigConfig`] component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRpoFalcon512MultisigConfig {
    /// List of procedures with authentication thresholds different than the default.
    pub proc_threshold_map: Vec<(Word, u32)>,
}

impl AuthRpoFalcon512MultisigConfig {
    /// Creates a new configuration with no threshold map.
    pub fn new(proc_threshold_map: Vec<(Word, u32)>) -> Self {
        Self { proc_threshold_map }
    }
}

impl Default for AuthRpoFalcon512MultisigConfig {
    fn default() -> Self {
        Self::new(vec![])
    }
}

/// An [`AccountComponent`] implementing a multisig based on RpoFalcon512 signatures.
///
/// This component requires a threshold number of signatures from a set of approvers.
///
/// The storage layout is:
/// - Slot 0(value): [threshold, num_approvers, 0, 0]
/// - Slot 1(map): A map with approver public keys (index -> pubkey)
/// - Slot 2(map): A map which stores executed transactions
/// - Slot 3(map): A map which stores procedure thresholds (PROC_ROOT -> threshold)
///
/// This component supports all account types.
#[derive(Debug)]
pub struct AuthRpoFalcon512Multisig {
    threshold: u32,
    approvers: Vec<PublicKeyCommitment>,
    config: AuthRpoFalcon512MultisigConfig,
}

impl AuthRpoFalcon512Multisig {
    /// Creates a new [`AuthRpoFalcon512Multisig`] component with the given `threshold` and
    /// list of approver public keys.
    ///
    /// # Errors
    /// Returns an error if threshold is 0 or greater than the number of approvers.
    pub fn new(
        threshold: u32,
        approvers: Vec<PublicKeyCommitment>,
        config: AuthRpoFalcon512MultisigConfig,
    ) -> Result<Self, AccountError> {
        if threshold == 0 {
            return Err(AccountError::other("threshold must be at least 1"));
        }

        if threshold > approvers.len() as u32 {
            return Err(AccountError::other(
                "threshold cannot be greater than number of approvers",
            ));
        }

        for (_, threshold) in &config.proc_threshold_map {
            if *threshold == 0 {
                return Err(AccountError::other("procedure threshold must be at least 1"));
            }

            if *threshold > approvers.len() as u32 {
                return Err(AccountError::other(
                    "procedure threshold cannot be greater than number of approvers",
                ));
            }
        }

        Ok(Self { threshold, approvers, config })
    }
}

impl From<AuthRpoFalcon512Multisig> for AccountComponent {
    fn from(multisig: AuthRpoFalcon512Multisig) -> Self {
        let mut storage_slots = Vec::with_capacity(3);

        // Slot 0: [threshold, num_approvers, 0, 0]
        let num_approvers = multisig.approvers.len() as u32;
        storage_slots.push(StorageSlot::Value(Word::from([
            multisig.threshold,
            num_approvers,
            0,
            0,
        ])));

        // Slot 1: A map with approver public keys
        let map_entries = multisig
            .approvers
            .iter()
            .enumerate()
            .map(|(i, pub_key)| (Word::from([i as u32, 0, 0, 0]), (*pub_key).into()));

        // Safe to unwrap because we know that the map keys are unique.
        storage_slots.push(StorageSlot::Map(StorageMap::with_entries(map_entries).unwrap()));

        // Slot 2: A map which stores executed transactions
        let executed_transactions = StorageMap::default();
        storage_slots.push(StorageSlot::Map(executed_transactions));

        // Slot 3: A map which stores procedure thresholds (PROC_ROOT -> threshold)
        let proc_threshold_roots = StorageMap::with_entries(
            multisig
                .config
                .proc_threshold_map
                .iter()
                .map(|(proc_root, threshold)| (*proc_root, Word::from([*threshold, 0, 0, 0]))),
        )
        .unwrap();
        storage_slots.push(StorageSlot::Map(proc_threshold_roots));

        AccountComponent::new(rpo_falcon_512_multisig_library(), storage_slots)
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
        let multisig_component = AuthRpoFalcon512Multisig::new(
            threshold,
            approvers.clone(),
            AuthRpoFalcon512MultisigConfig::default(),
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
                .get_map_item(1, Word::from([i as u32, 0, 0, 0]))
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

        let multisig_component = AuthRpoFalcon512Multisig::new(
            threshold,
            approvers.clone(),
            AuthRpoFalcon512MultisigConfig::default(),
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
            .get_map_item(1, Word::from([0u32, 0, 0, 0]))
            .expect("storage map access failed");
        assert_eq!(stored_pub_key, Word::from(pub_key));
    }

    /// Test multisig component error cases
    #[test]
    fn test_multisig_component_error_cases() {
        let pub_key = PublicKeyCommitment::from(Word::from([1u32, 0, 0, 0]));
        let approvers = vec![pub_key];

        // Test threshold = 0 (should fail)
        let result = AuthRpoFalcon512Multisig::new(
            0,
            approvers.clone(),
            AuthRpoFalcon512MultisigConfig::default(),
        );
        assert!(result.unwrap_err().to_string().contains("threshold must be at least 1"));

        // Test threshold > number of approvers (should fail)
        let result =
            AuthRpoFalcon512Multisig::new(2, approvers, AuthRpoFalcon512MultisigConfig::default());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("threshold cannot be greater than number of approvers")
        );
    }
}

use alloc::vec::Vec;

use miden_objects::account::{AccountCode, AccountComponent, StorageMap, StorageSlot};
use miden_objects::crypto::dsa::rpo_falcon512::PublicKey;
use miden_objects::{AccountError, Word};

use crate::account::components::{
    multisig_library,
    no_auth_library,
    rpo_falcon_512_acl_library,
    rpo_falcon_512_library,
};

/// An [`AccountComponent`] implementing the RpoFalcon512 signature scheme for authentication of
/// transactions.
///
/// It reexports the procedures from `miden::contracts::auth::basic`. When linking against this
/// component, the `miden` library (i.e. [`MidenLib`](crate::MidenLib)) must be available to the
/// assembler which is the case when using [`TransactionKernel::assembler()`][kasm]. The procedures
/// of this component are:
/// - `auth__tx_rpo_falcon512`, which can be used to verify a signature provided via the advice
///   stack to authenticate a transaction.
///
/// This component supports all account types.
///
/// [kasm]: crate::transaction::TransactionKernel::assembler
pub struct AuthRpoFalcon512 {
    public_key: PublicKey,
}

impl AuthRpoFalcon512 {
    /// Creates a new [`AuthRpoFalcon512`] component with the given `public_key`.
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl From<AuthRpoFalcon512> for AccountComponent {
    fn from(falcon: AuthRpoFalcon512) -> Self {
        AccountComponent::new(
            rpo_falcon_512_library(),
            vec![StorageSlot::Value(falcon.public_key.into())],
        )
        .expect("falcon component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
    }
}

/// Configuration for [`AuthRpoFalcon512Acl`] component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthRpoFalcon512AclConfig {
    /// List of procedure roots that require authentication when called.
    pub auth_trigger_procedures: Vec<Word>,
    /// When `false`, creating output notes (sending notes to other accounts) requires
    /// authentication. When `true`, output notes can be created without authentication.
    pub allow_unauthorized_output_notes: bool,
    /// When `false`, consuming input notes (processing notes sent to this account) requires
    /// authentication. When `true`, input notes can be consumed without authentication.
    pub allow_unauthorized_input_notes: bool,
}

impl AuthRpoFalcon512AclConfig {
    /// Creates a new configuration with no trigger procedures and both flags set to `false` (most
    /// restrictive).
    pub fn new() -> Self {
        Self {
            auth_trigger_procedures: vec![],
            allow_unauthorized_output_notes: false,
            allow_unauthorized_input_notes: false,
        }
    }

    /// Sets the list of procedure roots that require authentication when called.
    pub fn with_auth_trigger_procedures(mut self, procedures: Vec<Word>) -> Self {
        self.auth_trigger_procedures = procedures;
        self
    }

    /// Sets whether unauthorized output notes are allowed.
    pub fn with_allow_unauthorized_output_notes(mut self, allow: bool) -> Self {
        self.allow_unauthorized_output_notes = allow;
        self
    }

    /// Sets whether unauthorized input notes are allowed.
    pub fn with_allow_unauthorized_input_notes(mut self, allow: bool) -> Self {
        self.allow_unauthorized_input_notes = allow;
        self
    }
}

impl Default for AuthRpoFalcon512AclConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// An [`AccountComponent`] implementing a procedure-based Access Control List (ACL) using the
/// RpoFalcon512 signature scheme for authentication of transactions.
///
/// This component provides fine-grained authentication control based on three conditions:
/// 1. **Procedure-based authentication**: Requires authentication when any of the specified trigger
///    procedures are called during the transaction.
/// 2. **Output note authentication**: Controls whether creating output notes requires
///    authentication. Output notes are new notes created by the account and sent to other accounts
///    (e.g., when transferring assets). When `allow_unauthorized_output_notes` is `false`, any
///    transaction that creates output notes must be authenticated, ensuring account owners control
///    when their account sends assets to other accounts.
/// 3. **Input note authentication**: Controls whether consuming input notes requires
///    authentication. Input notes are notes that were sent to this account by other accounts (e.g.,
///    incoming asset transfers). When `allow_unauthorized_input_notes` is `false`, any transaction
///    that consumes input notes must be authenticated, ensuring account owners control when their
///    account processes incoming notes.
///
/// ## Authentication Logic
///
/// Authentication is required if ANY of the following conditions are true:
/// - Any trigger procedure from the ACL was called
/// - Output notes were created AND `allow_unauthorized_output_notes` is `false`
/// - Input notes were consumed AND `allow_unauthorized_input_notes` is `false`
///
/// If none of these conditions are met, only the nonce is incremented without requiring a
/// signature.
///
/// ## Use Cases
///
/// - **Restrictive mode** (`allow_unauthorized_output_notes=false`,
///   `allow_unauthorized_input_notes=false`): All note operations require authentication, providing
///   maximum security.
/// - **Selective mode**: Allow some note operations without authentication while protecting
///   specific procedures, useful for accounts that need to process certain operations
///   automatically.
/// - **Procedure-only mode** (`allow_unauthorized_output_notes=true`,
///   `allow_unauthorized_input_notes=true`): Only specific procedures require authentication,
///   allowing free note processing.
///
/// ## Storage Layout
/// - Slot 0(value): Public key (same as RpoFalcon512)
/// - Slot 1(value): [num_tracked_procs, allow_unauthorized_output_notes,
///   allow_unauthorized_input_notes, 0]
/// - Slot 2(map): A map with trigger procedure roots
///
/// ## Important Note on Procedure Detection
/// The procedure-based authentication relies on the `was_procedure_called` kernel function,
/// which only returns `true` if the procedure in question called into a kernel account API
/// that is restricted to the account context. Procedures that don't interact with account
/// state or kernel APIs may not be detected as "called" even if they were executed during
/// the transaction. This is an important limitation to consider when designing trigger
/// procedures for authentication.
///
/// This component supports all account types.
pub struct AuthRpoFalcon512Acl {
    public_key: PublicKey,
    config: AuthRpoFalcon512AclConfig,
}

impl AuthRpoFalcon512Acl {
    /// Creates a new [`AuthRpoFalcon512Acl`] component with the given `public_key` and
    /// configuration.
    ///
    /// # Panics
    /// Panics if more than [AccountCode::MAX_NUM_PROCEDURES] procedures are specified.
    pub fn new(
        public_key: PublicKey,
        config: AuthRpoFalcon512AclConfig,
    ) -> Result<Self, AccountError> {
        let max_procedures = AccountCode::MAX_NUM_PROCEDURES;
        if config.auth_trigger_procedures.len() > max_procedures {
            return Err(AccountError::other(format!(
                "Cannot track more than {max_procedures} procedures (account limit)"
            )));
        }

        Ok(Self { public_key, config })
    }
}

impl From<AuthRpoFalcon512Acl> for AccountComponent {
    fn from(falcon: AuthRpoFalcon512Acl) -> Self {
        let mut storage_slots = Vec::with_capacity(3);

        // Slot 0: Public key
        storage_slots.push(StorageSlot::Value(falcon.public_key.into()));

        // Slot 1: [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0]
        let num_procs = falcon.config.auth_trigger_procedures.len() as u32;
        storage_slots.push(StorageSlot::Value(Word::from([
            num_procs,
            falcon.config.allow_unauthorized_output_notes as u32,
            falcon.config.allow_unauthorized_input_notes as u32,
            0,
        ])));

        // Slot 2: A map with tracked procedure roots
        // We add the map even if there are no trigger procedures, to always maintain the same
        // storage layout.
        let map_entries = falcon
            .config
            .auth_trigger_procedures
            .iter()
            .enumerate()
            .map(|(i, proc_root)| (Word::from([i as u32, 0, 0, 0]), *proc_root));

        // Safe to unwrap because we know that the map keys are unique.
        storage_slots.push(StorageSlot::Map(StorageMap::with_entries(map_entries).unwrap()));

        AccountComponent::new(rpo_falcon_512_acl_library(), storage_slots)
            .expect(
                "ACL auth component should satisfy the requirements of a valid account component",
            )
            .with_supports_all_types()
    }
}

/// An [`AccountComponent`] implementing a no-authentication scheme.
///
/// This component provides **no authentication**! It only checks if the account
/// state has actually changed during transaction execution by comparing the initial
/// account commitment with the current commitment and increments the nonce if
/// they differ. This avoids unnecessary nonce increments for transactions that don't
/// modify the account state.
///
/// It exports the procedure `auth__no_auth`, which:
/// - Checks if the account state has changed by comparing initial and final commitments
/// - Only increments the nonce if the account state has actually changed
/// - Provides no cryptographic authentication
///
/// This component supports all account types.
pub struct NoAuth;

impl NoAuth {
    /// Creates a new [`NoAuth`] component.
    pub fn new() -> Self {
        Self
    }
}

impl Default for NoAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl From<NoAuth> for AccountComponent {
    fn from(_: NoAuth) -> Self {
        AccountComponent::new(no_auth_library(), vec![])
            .expect("NoAuth component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

// MULTISIG AUTHENTICATION COMPONENT
// ================================================================================================

/// An [`AccountComponent`] implementing a multisig based on RpoFalcon512 signatures.
///
/// This component requires a threshold number of signatures from a set of approvers.
///
/// The storage layout is:
/// - Slot 0(value): [threshold, num_approvers, 0, 0]
/// - Slot 1(map): A map with approver public keys (index -> pubkey)
/// - Slot 2(map): A map which stores executed transactions
///
/// This component supports all account types.
#[derive(Debug)]
pub struct AuthRpoFalcon512Multisig {
    threshold: u32,
    approvers: Vec<PublicKey>,
}

impl AuthRpoFalcon512Multisig {
    /// Creates a new [`AuthRpoFalcon512Multisig`] component with the given `threshold` and
    /// list of approver public keys.
    ///
    /// # Errors
    /// Returns an error if threshold is 0 or greater than the number of approvers.
    pub fn new(threshold: u32, approvers: Vec<PublicKey>) -> Result<Self, AccountError> {
        if threshold == 0 {
            return Err(AccountError::other("threshold must be at least 1"));
        }

        if threshold > approvers.len() as u32 {
            return Err(AccountError::other(
                "threshold cannot be greater than number of approvers",
            ));
        }

        Ok(Self { threshold, approvers })
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

        AccountComponent::new(multisig_library(), storage_slots)
            .expect("Multisig auth component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use alloc::string::ToString;

    use miden_objects::Word;
    use miden_objects::account::AccountBuilder;

    use super::*;
    use crate::account::components::WellKnownComponent;
    use crate::account::wallets::BasicWallet;

    /// Test configuration for parametrized ACL tests
    struct AclTestConfig {
        /// Whether to include auth trigger procedures
        with_procedures: bool,
        /// Allow unauthorized output notes flag
        allow_unauthorized_output_notes: bool,
        /// Allow unauthorized input notes flag
        allow_unauthorized_input_notes: bool,
        /// Expected slot 1 value [num_procs, allow_output, allow_input, 0]
        expected_slot_1: Word,
    }

    /// Helper function to get the basic wallet procedures for testing
    fn get_basic_wallet_procedures() -> Vec<Word> {
        // Get the two trigger procedures from BasicWallet: `receive_asset`, `move_asset_to_note`.
        let procedures: Vec<Word> = WellKnownComponent::BasicWallet.procedure_digests().collect();

        assert_eq!(procedures.len(), 2);
        procedures
    }

    /// Parametrized test helper for ACL component testing
    fn test_acl_component(config: AclTestConfig) {
        let public_key = PublicKey::new(Word::empty());

        // Build the configuration
        let mut acl_config = AuthRpoFalcon512AclConfig::new()
            .with_allow_unauthorized_output_notes(config.allow_unauthorized_output_notes)
            .with_allow_unauthorized_input_notes(config.allow_unauthorized_input_notes);

        let auth_trigger_procedures = if config.with_procedures {
            let procedures = get_basic_wallet_procedures();
            acl_config = acl_config.with_auth_trigger_procedures(procedures.clone());
            procedures
        } else {
            vec![]
        };

        // Create component and account
        let component =
            AuthRpoFalcon512Acl::new(public_key, acl_config).expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        // Assert public key in slot 0
        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key));

        // Assert configuration in slot 1
        let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
        assert_eq!(slot_1, config.expected_slot_1);

        // Assert procedure roots in map (slot 2)
        if config.with_procedures {
            for (i, expected_proc_root) in auth_trigger_procedures.iter().enumerate() {
                let proc_root = account
                    .storage()
                    .get_map_item(2, Word::from([i as u32, 0, 0, 0]))
                    .expect("storage map access failed");
                assert_eq!(proc_root, *expected_proc_root);
            }
        } else {
            // When no procedures, the map should return empty for key [0,0,0,0]
            let proc_root = account
                .storage()
                .get_map_item(2, Word::empty())
                .expect("storage map access failed");
            assert_eq!(proc_root, Word::empty());
        }
    }

    /// Test ACL component with no procedures and both authorization flags set to false
    #[test]
    fn test_rpo_falcon_512_acl_no_procedures() {
        test_acl_component(AclTestConfig {
            with_procedures: false,
            allow_unauthorized_output_notes: false,
            allow_unauthorized_input_notes: false,
            expected_slot_1: Word::empty(), // [0, 0, 0, 0]
        });
    }

    /// Test ACL component with two procedures and both authorization flags set to false
    #[test]
    fn test_rpo_falcon_512_acl_with_two_procedures() {
        test_acl_component(AclTestConfig {
            with_procedures: true,
            allow_unauthorized_output_notes: false,
            allow_unauthorized_input_notes: false,
            expected_slot_1: Word::from([2u32, 0, 0, 0]),
        });
    }

    /// Test ACL component with no procedures and allow_unauthorized_output_notes set to true
    #[test]
    fn test_rpo_falcon_512_acl_with_allow_unauthorized_output_notes() {
        test_acl_component(AclTestConfig {
            with_procedures: false,
            allow_unauthorized_output_notes: true,
            allow_unauthorized_input_notes: false,
            expected_slot_1: Word::from([0u32, 1, 0, 0]),
        });
    }

    /// Test ACL component with two procedures and allow_unauthorized_output_notes set to true
    #[test]
    fn test_rpo_falcon_512_acl_with_procedures_and_allow_unauthorized_output_notes() {
        test_acl_component(AclTestConfig {
            with_procedures: true,
            allow_unauthorized_output_notes: true,
            allow_unauthorized_input_notes: false,
            expected_slot_1: Word::from([2u32, 1, 0, 0]),
        });
    }

    /// Test ACL component with no procedures and allow_unauthorized_input_notes set to true
    #[test]
    fn test_rpo_falcon_512_acl_with_allow_unauthorized_input_notes() {
        test_acl_component(AclTestConfig {
            with_procedures: false,
            allow_unauthorized_output_notes: false,
            allow_unauthorized_input_notes: true,
            expected_slot_1: Word::from([0u32, 0, 1, 0]),
        });
    }

    /// Test ACL component with two procedures and both authorization flags set to true
    #[test]
    fn test_rpo_falcon_512_acl_with_both_allow_flags() {
        test_acl_component(AclTestConfig {
            with_procedures: true,
            allow_unauthorized_output_notes: true,
            allow_unauthorized_input_notes: true,
            expected_slot_1: Word::from([2u32, 1, 1, 0]),
        });
    }

    #[test]
    fn test_no_auth_component() {
        // Create an account using the NoAuth component
        let (_account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(NoAuth)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");
    }

    // MULTISIG TESTS
    // ============================================================================================

    /// Test multisig component setup with various configurations
    #[test]
    fn test_multisig_component_setup() {
        // Create test public keys
        let pub_key_1 = PublicKey::new(Word::from([1u32, 0, 0, 0]));
        let pub_key_2 = PublicKey::new(Word::from([2u32, 0, 0, 0]));
        let pub_key_3 = PublicKey::new(Word::from([3u32, 0, 0, 0]));
        let approvers = vec![pub_key_1, pub_key_2, pub_key_3];
        let threshold = 2u32;

        // Create multisig component
        let multisig_component = AuthRpoFalcon512Multisig::new(threshold, approvers.clone())
            .expect("multisig component creation failed");

        // Build account with multisig component
        let (account, _) = AccountBuilder::new([0; 32])
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
        let pub_key = PublicKey::new(Word::from([42u32, 0, 0, 0]));
        let approvers = vec![pub_key];
        let threshold = 1u32;

        let multisig_component = AuthRpoFalcon512Multisig::new(threshold, approvers.clone())
            .expect("multisig component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
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
        let pub_key = PublicKey::new(Word::from([1u32, 0, 0, 0]));
        let approvers = vec![pub_key];

        // Test threshold = 0 (should fail)
        let result = AuthRpoFalcon512Multisig::new(0, approvers.clone());
        assert!(result.unwrap_err().to_string().contains("threshold must be at least 1"));

        // Test threshold > number of approvers (should fail)
        let result = AuthRpoFalcon512Multisig::new(2, approvers);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("threshold cannot be greater than number of approvers")
        );
    }
}

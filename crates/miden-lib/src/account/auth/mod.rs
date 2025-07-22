use alloc::{string::ToString, vec::Vec};

use miden_objects::{
    AccountError, Word,
    account::{AccountCode, AccountComponent, StorageMap, StorageSlot},
    crypto::dsa::rpo_falcon512::PublicKey,
};

use crate::account::components::{rpo_falcon_512_library, rpo_falcon_512_procedure_acl_library};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthRpoFalcon512AclConfig {
    /// When `false`, creating output notes (sending notes to other accounts) requires
    /// authentication. When `true`, output notes can be created without authentication.
    pub allow_unauthorized_output_notes: bool,
    /// When `false`, consuming input notes (processing notes sent to this account) requires
    /// authentication. When `true`, input notes can be consumed without authentication.
    pub allow_unauthorized_input_notes: bool,
}

impl AuthRpoFalcon512AclConfig {
    /// Creates a new configuration with both flags set to `false` (most restrictive).
    pub fn new() -> Self {
        Self {
            allow_unauthorized_output_notes: false,
            allow_unauthorized_input_notes: false,
        }
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
    auth_trigger_procedures: Vec<Word>,
    config: AuthRpoFalcon512AclConfig,
}

impl AuthRpoFalcon512Acl {
    /// Creates a new [`AuthRpoFalcon512Acl`] component with the given `public_key`,
    /// list of procedure roots that require authentication, and configuration for note
    /// authorization.
    ///
    /// # Panics
    /// Panics if more than [AccountCode::MAX_NUM_PROCEDURES] procedures are specified.
    pub fn new(
        public_key: PublicKey,
        auth_trigger_procedures: Vec<Word>,
        config: AuthRpoFalcon512AclConfig,
    ) -> Result<Self, AccountError> {
        let max_procedures = AccountCode::MAX_NUM_PROCEDURES;
        if auth_trigger_procedures.len() > max_procedures {
            return Err(AccountError::AssumptionViolated(
                "Cannot track more than {max_procedures} procedures (account limit)".to_string(),
            ));
        }

        Ok(Self {
            public_key,
            auth_trigger_procedures,
            config,
        })
    }

    /// Creates a new [`AuthRpoFalcon512Acl`] component with the given `public_key`,
    /// list of procedure roots that require authentication, and boolean flags for note
    /// authorization.
    ///
    /// This method is provided for backward compatibility. Consider using [`Self::new`] with
    /// [`AuthRpoFalcon512AclConfig`] for better readability.
    #[deprecated(since = "0.11.0", note = "Use `new` with `AuthRpoFalcon512AclConfig` instead")]
    pub fn new_with_flags(
        public_key: PublicKey,
        auth_trigger_procedures: Vec<Word>,
        allow_unauthorized_output_notes: bool,
        allow_unauthorized_input_notes: bool,
    ) -> Result<Self, AccountError> {
        let config = AuthRpoFalcon512AclConfig {
            allow_unauthorized_output_notes,
            allow_unauthorized_input_notes,
        };
        Self::new(public_key, auth_trigger_procedures, config)
    }
}

impl From<AuthRpoFalcon512Acl> for AccountComponent {
    fn from(falcon: AuthRpoFalcon512Acl) -> Self {
        let mut storage_slots = Vec::with_capacity(3);

        // Slot 0: Public key
        storage_slots.push(StorageSlot::Value(falcon.public_key.into()));

        // Slot 1: [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0]
        let num_procs = falcon.auth_trigger_procedures.len() as u32;
        storage_slots.push(StorageSlot::Value(Word::from([
            num_procs,
            u32::from(falcon.config.allow_unauthorized_output_notes),
            u32::from(falcon.config.allow_unauthorized_input_notes),
            0,
        ])));

        // Slot 2: A map with tracked procedure roots
        // We add the map even if there are no trigger procedures, to always maintain the same
        // storage layout.
        let map_entries = falcon
            .auth_trigger_procedures
            .iter()
            .enumerate()
            .map(|(i, proc_root)| (Word::from([i as u32, 0, 0, 0]), *proc_root));

        // Safe to unwrap because we know that the map keys are unique.
        storage_slots.push(StorageSlot::Map(StorageMap::with_entries(map_entries).unwrap()));

        AccountComponent::new(rpo_falcon_512_procedure_acl_library(), storage_slots)
            .expect("Procedure ACL auth component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use miden_objects::{Word, account::AccountBuilder};

    use super::*;
    use crate::account::{components::basic_wallet_library, wallets::BasicWallet};

    #[test]
    fn test_rpo_falcon_512_procedure_acl_no_procedures() {
        let public_key = PublicKey::new(Word::empty());
        let component =
            AuthRpoFalcon512Acl::new(public_key, vec![], AuthRpoFalcon512AclConfig::new())
                .expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key));

        let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
        // Slot 1 should be [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0] With 0 procedures,
        // allow_unauthorized_output_notes=false, and allow_unauthorized_input_notes=false, this
        // should be [0, 0, 0, 0]
        assert_eq!(slot_1, Word::empty());

        let proc_root = account
            .storage()
            .get_map_item(2, Word::empty())
            .expect("storage map access failed");
        // This should be filled with zeros because there are no auth trigger procedures
        assert_eq!(proc_root, Word::empty());
    }

    #[test]
    fn test_rpo_falcon_512_procedure_acl_with_two_procedures() {
        let public_key = PublicKey::new(Word::empty());

        // Get the two trigger procedures from BasicWallet: `receive_asset`, `move_asset_to_note`.
        // TODO refactor to fetch procedure digests by name after
        // https://github.com/0xMiden/miden-base/pull/1532
        let auth_trigger_procedures: Vec<Word> = basic_wallet_library()
            .module_infos()
            .next()
            .expect("at least one module expected")
            .procedures()
            .map(|(_, proc_info)| proc_info.digest)
            .collect();

        assert_eq!(auth_trigger_procedures.len(), 2);

        let component = AuthRpoFalcon512Acl::new(
            public_key,
            auth_trigger_procedures.clone(),
            AuthRpoFalcon512AclConfig::new(),
        )
        .expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key));

        let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
        // Slot 1 should be [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0] With 2 procedures,
        // allow_unauthorized_output_notes=false, and allow_unauthorized_input_notes=false, this
        // should be [2, 0, 0, 0]
        assert_eq!(slot_1, Word::from([2u32, 0, 0, 0]));

        let proc_root_0 = account
            .storage()
            .get_map_item(2, Word::empty())
            .expect("storage map access failed");
        assert_eq!(proc_root_0, auth_trigger_procedures[0]);

        let proc_root_1 = account
            .storage()
            .get_map_item(2, Word::from([1, 0, 0, 0u32]))
            .expect("storage map access failed");
        assert_eq!(proc_root_1, auth_trigger_procedures[1]);
    }

    #[test]
    fn test_rpo_falcon_512_procedure_acl_with_allow_unauthorized_output_notes() {
        let public_key = PublicKey::new(Word::empty());
        let component = AuthRpoFalcon512Acl::new(
            public_key,
            vec![],
            AuthRpoFalcon512AclConfig::new().with_allow_unauthorized_output_notes(true),
        )
        .expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key));

        let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
        // Slot 1 should be [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0] With 0 procedures,
        // allow_unauthorized_output_notes=true, and allow_unauthorized_input_notes=false, this
        // should be [0, 1, 0, 0]
        assert_eq!(slot_1, Word::from([0, 1, 0, 0u32]));
    }

    #[test]
    fn test_rpo_falcon_512_procedure_acl_with_procedures_and_allow_unauthorized_output_notes() {
        let public_key = PublicKey::new(Word::empty());

        // Get the two trigger procedures from BasicWallet: `receive_asset`, `move_asset_to_note`.
        let auth_trigger_procedures: Vec<Word> = basic_wallet_library()
            .module_infos()
            .next()
            .expect("at least one module expected")
            .procedures()
            .map(|(_, proc_info)| proc_info.digest)
            .collect();

        assert_eq!(auth_trigger_procedures.len(), 2);

        let component = AuthRpoFalcon512Acl::new(
            public_key,
            auth_trigger_procedures.clone(),
            AuthRpoFalcon512AclConfig::new().with_allow_unauthorized_output_notes(true),
        )
        .expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key));

        let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
        // Slot 1 should be [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0] With 2 procedures,
        // allow_unauthorized_output_notes=true, and allow_unauthorized_input_notes=false, this
        // should be [2, 1, 0, 0]
        assert_eq!(slot_1, Word::from([2u32, 1, 0, 0]));

        let proc_root_0 = account
            .storage()
            .get_map_item(2, Word::empty())
            .expect("storage map access failed");
        assert_eq!(proc_root_0, auth_trigger_procedures[0]);

        let proc_root_1 = account
            .storage()
            .get_map_item(2, Word::from([1, 0, 0, 0u32]))
            .expect("storage map access failed");
        assert_eq!(proc_root_1, auth_trigger_procedures[1]);
    }

    #[test]
    fn test_rpo_falcon_512_procedure_acl_with_allow_unauthorized_input_notes() {
        let public_key = PublicKey::new(Word::empty());
        let component = AuthRpoFalcon512Acl::new(
            public_key,
            vec![],
            AuthRpoFalcon512AclConfig::new().with_allow_unauthorized_input_notes(true),
        )
        .expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key));

        let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
        // Slot 1 should be [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0] With 0 procedures,
        // allow_unauthorized_output_notes=false, and allow_unauthorized_input_notes=true, this
        // should be [0, 0, 1, 0]
        assert_eq!(slot_1, Word::from([0, 0, 1, 0u32]));
    }

    #[test]
    fn test_rpo_falcon_512_procedure_acl_with_both_allow_flags() {
        let public_key = PublicKey::new(Word::empty());

        // Get the two trigger procedures from BasicWallet: `receive_asset`, `move_asset_to_note`.
        let auth_trigger_procedures: Vec<Word> = basic_wallet_library()
            .module_infos()
            .next()
            .expect("at least one module expected")
            .procedures()
            .map(|(_, proc_info)| proc_info.digest)
            .collect();

        assert_eq!(auth_trigger_procedures.len(), 2);

        let component = AuthRpoFalcon512Acl::new(
            public_key,
            auth_trigger_procedures.clone(),
            AuthRpoFalcon512AclConfig::new()
                .with_allow_unauthorized_output_notes(true)
                .with_allow_unauthorized_input_notes(true),
        )
        .expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key));

        let slot_1 = account.storage().get_item(1).expect("storage slot 1 access failed");
        // Slot 1 should be [num_tracked_procs, allow_unauthorized_output_notes,
        // allow_unauthorized_input_notes, 0] With 2 procedures,
        // allow_unauthorized_output_notes=true, and allow_unauthorized_input_notes=true, this
        // should be [2, 1, 1, 0]
        assert_eq!(slot_1, Word::from([2u32, 1, 1, 0]));

        let proc_root_0 = account
            .storage()
            .get_map_item(2, Word::empty())
            .expect("storage map access failed");
        assert_eq!(proc_root_0, auth_trigger_procedures[0]);

        let proc_root_1 = account
            .storage()
            .get_map_item(2, Word::from([1, 0, 0, 0u32]))
            .expect("storage map access failed");
        assert_eq!(proc_root_1, auth_trigger_procedures[1]);
    }
}

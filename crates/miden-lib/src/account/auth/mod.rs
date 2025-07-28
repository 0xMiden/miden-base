use alloc::{string::ToString, vec::Vec};

use miden_objects::{
    AccountError, Digest, Felt, FieldElement,
    account::{AccountCode, AccountComponent, StorageMap, StorageSlot},
    crypto::dsa::rpo_falcon512::PublicKey,
};

use crate::account::components::{
    no_auth_library, rpo_falcon_512_library, rpo_falcon_512_procedure_acl_library,
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
pub struct RpoFalcon512 {
    public_key: PublicKey,
}

impl RpoFalcon512 {
    /// Creates a new [`RpoFalcon512`] component with the given `public_key`.
    pub fn new(public_key: PublicKey) -> Self {
        Self { public_key }
    }
}

impl From<RpoFalcon512> for AccountComponent {
    fn from(falcon: RpoFalcon512) -> Self {
        AccountComponent::new(
            rpo_falcon_512_library(),
            vec![StorageSlot::Value(falcon.public_key.into())],
        )
        .expect("falcon component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
    }
}

/// An [`AccountComponent`] implementing a procedure-ACL based RpoFalcon512 signature scheme for
/// authentication of transactions.
///
/// This component only requires authentication when any of the specified procedures are called
/// during the transaction. It stores a list of procedure roots that require authentication, and
/// the signature verification is only performed if at least one of these procedures was invoked.
///
/// It exports the procedure `auth__tx_rpo_falcon512_procedure_acl`, which:
/// - Checks if any of the specified auth trigger procedures were called during the transaction
/// - If none were called, authentication is skipped
/// - If at least one was called, performs standard RpoFalcon512 signature verification
/// - Always increments the nonce
///
/// The storage layout is:
/// - Slot 0(value): Public key (same as RpoFalcon512)
/// - Slot 1(value): Number of trigger procedures
/// - Slot 2(map): A map with trigger procedure roots
///
/// This component supports all account types.
pub struct RpoFalcon512ProcedureAcl {
    public_key: PublicKey,
    auth_trigger_procedures: Vec<Digest>,
}

impl RpoFalcon512ProcedureAcl {
    /// Creates a new [`RpoFalcon512ProcedureAcl`] component with the given `public_key` and
    /// list of procedure roots that require authentication.
    ///
    /// # Panics
    /// Panics if more than [AccountCode::MAX_NUM_PROCEDURES] procedures are specified.
    pub fn new(
        public_key: PublicKey,
        auth_trigger_procedures: Vec<Digest>,
    ) -> Result<Self, AccountError> {
        let max_procedures = AccountCode::MAX_NUM_PROCEDURES;
        if auth_trigger_procedures.len() > max_procedures {
            return Err(AccountError::AssumptionViolated(
                "Cannot track more than {max_procedures} procedures (account limit)".to_string(),
            ));
        }

        Ok(Self { public_key, auth_trigger_procedures })
    }
}

impl From<RpoFalcon512ProcedureAcl> for AccountComponent {
    fn from(falcon: RpoFalcon512ProcedureAcl) -> Self {
        let mut storage_slots = Vec::with_capacity(3);

        // Slot 0: Public key
        storage_slots.push(StorageSlot::Value(falcon.public_key.into()));

        // Slot 1: Number of tracked procedures
        let num_procs = Felt::from(falcon.auth_trigger_procedures.len() as u32);
        storage_slots.push(StorageSlot::Value([num_procs, Felt::ZERO, Felt::ZERO, Felt::ZERO]));

        // Slot 2: A map with tracked procedure roots
        // We add the map even if there are no trigger procedures, to always maintain the same
        // storage layout.
        let map_entries =
            falcon.auth_trigger_procedures.iter().enumerate().map(|(i, proc_root)| {
                (
                    [Felt::from(i as u32), Felt::ZERO, Felt::ZERO, Felt::ZERO].into(),
                    proc_root.into(),
                )
            });

        // Safe to unwrap because we know that the map keys are unique.
        storage_slots.push(StorageSlot::Map(StorageMap::with_entries(map_entries).unwrap()));

        AccountComponent::new(rpo_falcon_512_procedure_acl_library(), storage_slots)
            .expect("Procedure ACL auth component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

/// An [`AccountComponent`] that provides no authentication - it only increments the nonce.
///
/// This component exports the procedure `auth__none`, which simply increments the account nonce
/// without performing any signature verification. This is useful for testing scenarios or accounts
/// that don't require authentication.
///
/// This component supports all account types.
pub struct AuthNone;

impl From<AuthNone> for AccountComponent {
    fn from(_: AuthNone) -> Self {
        AccountComponent::new(no_auth_library(), vec![])
            .expect(
                "AuthNone component should satisfy the requirements of a valid account component",
            )
            .with_supports_all_types()
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use miden_objects::{Word, ZERO, account::AccountBuilder};

    use super::*;
    use crate::account::{components::basic_wallet_library, wallets::BasicWallet};

    #[test]
    fn test_rpo_falcon_512_procedure_acl_no_procedures() {
        let public_key = PublicKey::new([ZERO; 4]);
        let component =
            RpoFalcon512ProcedureAcl::new(public_key, vec![]).expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key).into());

        let num_procs_slot = account.storage().get_item(1).expect("storage slot 1 access failed");
        assert_eq!(num_procs_slot, [Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::ZERO].into());

        let proc_root = account
            .storage()
            .get_map_item(2, [Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::ZERO])
            .expect("storage map access failed");
        // This should be filled with zeros because there are no auth trigger procedures
        assert_eq!(proc_root, Word::default());
    }

    #[test]
    fn test_rpo_falcon_512_procedure_acl_with_two_procedures() {
        let public_key = PublicKey::new([ZERO; 4]);

        // Get the two trigger procedures from BasicWallet: `receive_asset`, `move_asset_to_note`.
        // TODO refactor to fetch procedure digests by name after
        // https://github.com/0xMiden/miden-base/pull/1532
        let auth_trigger_procedures: Vec<Digest> = basic_wallet_library()
            .module_infos()
            .next()
            .expect("at least one module expected")
            .procedures()
            .map(|(_, proc_info)| proc_info.digest)
            .collect();

        assert_eq!(auth_trigger_procedures.len(), 2);

        let component = RpoFalcon512ProcedureAcl::new(public_key, auth_trigger_procedures.clone())
            .expect("component creation failed");

        let (account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(component)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");

        let public_key_slot = account.storage().get_item(0).expect("storage slot 0 access failed");
        assert_eq!(public_key_slot, Word::from(public_key).into());

        let num_procs_slot = account.storage().get_item(1).expect("storage slot 1 access failed");
        assert_eq!(num_procs_slot, [Felt::new(2), Felt::ZERO, Felt::ZERO, Felt::ZERO].into());

        let proc_root_0 = account
            .storage()
            .get_map_item(2, [Felt::ZERO, Felt::ZERO, Felt::ZERO, Felt::ZERO])
            .expect("storage map access failed");
        assert_eq!(proc_root_0, Word::from(auth_trigger_procedures[0]));

        let proc_root_1 = account
            .storage()
            .get_map_item(2, [Felt::ONE, Felt::ZERO, Felt::ZERO, Felt::ZERO])
            .expect("storage map access failed");
        assert_eq!(proc_root_1, Word::from(auth_trigger_procedures[1]));
    }

    #[test]
    fn test_auth_none_component() {
        // Create an account using the AuthNone component
        let (_account, _) = AccountBuilder::new([0; 32])
            .with_auth_component(AuthNone)
            .with_component(BasicWallet)
            .build()
            .expect("account building failed");
    }
}

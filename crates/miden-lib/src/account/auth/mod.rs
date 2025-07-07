use alloc::vec::Vec;

use miden_objects::{
    Digest, Felt, FieldElement,
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

/// An [`AccountComponent`] implementing a conditional RpoFalcon512 signature scheme for
/// authentication of transactions.
///
/// This component only requires authentication when any of the specified procedures are called
/// during the transaction. It stores a list of procedure roots that require authentication, and
/// the signature verification is only performed if at least one of these procedures was invoked.
///
/// It exports the procedure `auth__tx_rpo_falcon512_procedures_acl`, which:
/// - Checks if any of the specified procedures were called during the transaction
/// - If none were called, authentication is skipped
/// - If at least one was called, performs standard RpoFalcon512 signature verification
///
/// The storage layout is:
/// - Slot 0(value): Public key (same as RpoFalcon512)
/// - Slot 1(value): Number of tracked procedures
/// - Slot 2(map): A map with tracked procedure roots
///
/// This component supports all account types.
pub struct RpoFalcon512ProcedureAcl {
    public_key: PublicKey,
    trigger_procedures: Vec<Digest>,
}

impl RpoFalcon512ProcedureAcl {
    /// Creates a new [`RpoFalcon512ProcedureAcl`] component with the given `public_key` and
    /// list of procedure roots that require authentication.
    ///
    /// # Panics
    /// Panics if more than 256 procedures are tracked.
    pub fn new(public_key: PublicKey, trigger_procedures: Vec<Digest>) -> Self {
        assert!(
            trigger_procedures.len() <= AccountCode::MAX_NUM_PROCEDURES,
            "Cannot track more than {} procedures (account's maximum limit)",
            AccountCode::MAX_NUM_PROCEDURES
        );
        Self { public_key, trigger_procedures }
    }
}

impl From<RpoFalcon512ProcedureAcl> for AccountComponent {
    fn from(conditional: RpoFalcon512ProcedureAcl) -> Self {
        let mut storage_slots = Vec::with_capacity(3);

        // Slot 0: Public key
        storage_slots.push(StorageSlot::Value(conditional.public_key.into()));

        // Slot 1: Number of tracked procedures
        let num_procs = Felt::from(conditional.trigger_procedures.len() as u32);
        storage_slots.push(StorageSlot::Value([num_procs, Felt::ZERO, Felt::ZERO, Felt::ZERO]));

        // Slot 2: A map with tracked procedure roots
        if !conditional.trigger_procedures.is_empty() {
            let map_entries: Vec<_> = conditional
                .trigger_procedures
                .iter()
                .enumerate()
                .map(|(i, proc_root)| {
                    (
                        [Felt::from(i as u32), Felt::ZERO, Felt::ZERO, Felt::ZERO].into(),
                        proc_root.into(),
                    )
                })
                .collect();

            storage_slots.push(StorageSlot::Map(StorageMap::with_entries(map_entries).unwrap()));
        }

        AccountComponent::new(rpo_falcon_512_procedure_acl_library(), storage_slots)
            .expect("falcon procedures ACL component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

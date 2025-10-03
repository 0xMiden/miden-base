use miden_objects::account::{AccountComponent, PublicKeyCommitment, StorageSlot};

use crate::account::components::rpo_falcon_512_library;

/// An [`AccountComponent`] implementing the RpoFalcon512 signature scheme for authentication of
/// transactions.
///
/// It reexports the procedures from `miden::contracts::auth::basic`. When linking against this
/// component, the `miden` library (i.e. [`MidenLib`](crate::MidenLib)) must be available to the
/// assembler which is the case when using [`TransactionKernel::assembler()`][kasm]. The procedures
/// of this component are:
/// - `auth_tx_rpo_falcon512`, which can be used to verify a signature provided via the advice stack
///   to authenticate a transaction.
///
/// This component supports all account types.
///
/// [kasm]: crate::transaction::TransactionKernel::assembler
pub struct AuthRpoFalcon512 {
    pub_key: PublicKeyCommitment,
}

impl AuthRpoFalcon512 {
    /// Creates a new [`AuthRpoFalcon512`] component with the given `public_key`.
    pub fn new(pub_key: PublicKeyCommitment) -> Self {
        Self { pub_key }
    }
}

impl From<AuthRpoFalcon512> for AccountComponent {
    fn from(falcon: AuthRpoFalcon512) -> Self {
        AccountComponent::new(
            rpo_falcon_512_library(),
            vec![StorageSlot::Value(falcon.pub_key.into())],
        )
        .expect("falcon component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
    }
}

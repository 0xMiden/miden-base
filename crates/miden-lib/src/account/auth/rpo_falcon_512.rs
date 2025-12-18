use miden_objects::account::auth::PublicKeyCommitment;
use miden_objects::account::{AccountComponent, StorageSlot, StorageSlotName};
use miden_objects::utils::sync::LazyLock;

use crate::account::components::rpo_falcon_512_library;

static FALCON_PUBKEY_SLOT_NAME: LazyLock<StorageSlotName> = LazyLock::new(|| {
    StorageSlotName::new("miden::standards::auth::rpo_falcon512::public_key")
        .expect("storage slot name should be valid")
});

/// An [`AccountComponent`] implementing the RpoFalcon512 signature scheme for authentication of
/// transactions.
///
/// It reexports the procedures from `miden::contracts::auth::rpo_falcon512`. When linking against
/// this component, the `miden` library (i.e. [`ProtocoLib`](miden_objects::ProtocolLib)) must
/// be available to the assembler which is the case when using [`CodeBuilder`][builder]. The
/// procedures of this component are:
/// - `verify_signatures`, which can be used to verify a signature provided via the advice stack to
///   authenticate a transaction.
/// - `authenticate_transaction`, which can be used to authenticate a transaction using the Falcon
///   signature scheme.
///
/// This component supports all account types.
///
/// ## Storage Layout
///
/// - [`Self::public_key_slot`]: Public key
///
/// [builder]: crate::code_builder::CodeBuilder
pub struct AuthRpoFalcon512 {
    pub_key: PublicKeyCommitment,
}

impl AuthRpoFalcon512 {
    /// Creates a new [`AuthRpoFalcon512`] component with the given `public_key`.
    pub fn new(pub_key: PublicKeyCommitment) -> Self {
        Self { pub_key }
    }

    /// Returns the [`StorageSlotName`] where the public key is stored.
    pub fn public_key_slot() -> &'static StorageSlotName {
        &FALCON_PUBKEY_SLOT_NAME
    }
}

impl From<AuthRpoFalcon512> for AccountComponent {
    fn from(falcon: AuthRpoFalcon512) -> Self {
        AccountComponent::new(
            rpo_falcon_512_library(),
            vec![StorageSlot::with_value(
                AuthRpoFalcon512::public_key_slot().clone(),
                falcon.pub_key.into(),
            )],
        )
        .expect("falcon component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
    }
}

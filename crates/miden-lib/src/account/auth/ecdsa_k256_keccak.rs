use miden_objects::account::auth::PublicKeyCommitment;
use miden_objects::account::{AccountComponent, StorageSlot, StorageSlotName};
use miden_objects::utils::sync::LazyLock;

use crate::account::components::ecdsa_k256_keccak_library;

static ECDSA_PUBKEY_SLOT_NAME: LazyLock<StorageSlotName> = LazyLock::new(|| {
    StorageSlotName::new("miden::standards::auth::ecdsa_k256_keccak::public_key")
        .expect("storage slot name should be valid")
});

/// An [`AccountComponent`] implementing the ECDSA K256 Keccak signature scheme for authentication
/// of transactions.
///
/// It reexports the procedures from `miden::contracts::auth::basic`. When linking against this
/// component, the `miden` library (i.e. [`MidenLib`](crate::MidenLib)) must be available to the
/// assembler which is the case when using [`ProtocolAssembler`][builder]. The procedures
/// of this component are:
/// - `auth_tx_ecdsa_k256_keccak`, which can be used to verify a signature provided via the advice
///   stack to authenticate a transaction.
///
/// This component supports all account types.
///
/// [builder]: crate::utils::ProtocolAssembler
pub struct AuthEcdsaK256Keccak {
    pub_key: PublicKeyCommitment,
}

impl AuthEcdsaK256Keccak {
    /// Creates a new [`AuthEcdsaK256Keccak`] component with the given `public_key`.
    pub fn new(pub_key: PublicKeyCommitment) -> Self {
        Self { pub_key }
    }

    /// Returns the [`StorageSlotName`] where the public key is stored.
    pub fn public_key_slot() -> &'static StorageSlotName {
        &ECDSA_PUBKEY_SLOT_NAME
    }
}

impl From<AuthEcdsaK256Keccak> for AccountComponent {
    fn from(ecdsa: AuthEcdsaK256Keccak) -> Self {
        AccountComponent::new(
            ecdsa_k256_keccak_library(),
            vec![StorageSlot::with_value(
                AuthEcdsaK256Keccak::public_key_slot().clone(),
                ecdsa.pub_key.into(),
            )],
        )
        .expect("ecdsa component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
    }
}

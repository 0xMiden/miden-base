use miden_protocol::Word;
use miden_protocol::account::auth::{AuthScheme, PublicKey};
use miden_protocol::account::component::{
    AccountComponentCode,
    AccountComponentMetadata,
    SchemaType,
    StorageSchema,
    StorageSlotSchema,
};
use miden_protocol::account::{
    AccountComponent,
    AccountComponentName,
    StorageSlot,
    StorageSlotName,
};
use miden_protocol::crypto::dsa::{ecdsa_k256_keccak, falcon512_poseidon2};
use miden_protocol::utils::sync::LazyLock;

use super::Approver;
use crate::account::account_component_code;

account_component_code!(AUTH_PASS_THROUGH_CODE, "miden-standards-auth-pass-through.masp");

// CONSTANTS
// ================================================================================================

static PUBKEY_SLOT_NAME: LazyLock<StorageSlotName> = LazyLock::new(|| {
    StorageSlotName::new("miden::standards::auth::pass_through::pub_key")
        .expect("storage slot name should be valid")
});

static SCHEME_ID_SLOT_NAME: LazyLock<StorageSlotName> = LazyLock::new(|| {
    StorageSlotName::new("miden::standards::auth::pass_through::scheme")
        .expect("storage slot name should be valid")
});

/// An [`AccountComponent`] implementing the authentication scheme of a pass-through account.
///
/// This component makes an account immutable: after the transaction that creates it, any
/// transaction that would change its commitment fails and the nonce is never incremented. Nothing
/// can therefore alter the account, so transactions against it never conflict with one another and
/// can be built concurrently.
///
/// It exports the procedure `auth_pass_through`, which:
/// - Verifies a signature over the transaction summary against the public key in storage, under the
///   signature scheme stored alongside it
/// - Asserts the account's commitment is the one it had at the start of the transaction
/// - Never increments the nonce, except once when the account is deployed (the kernel rejects a
///   final nonce of 0). That transaction is checked against the vault instead of the full
///   commitment, so it cannot be created holding assets
/// - Creates no TX_FEE note
///
/// Replay protection comes from the input notes the signed transaction summary binds, not from the
/// nonce; see [`AuthSingleSig`](crate::account::auth::AuthSingleSig) for the usual scheme. Since a
/// transaction leaving the account unchanged must consume at least one input note to be valid,
/// this always applies. The deploying transaction is exempt: an account can only be created once.
///
/// When linking against this component, the `miden::standards` library must be available to the
/// assembler (which also implies availability of `miden::protocol`). This is the case when using
/// [`CodeBuilder`][builder].
///
/// [builder]: crate::code_builder::CodeBuilder
pub struct AuthPassThrough {
    approver: Approver,
}

impl AuthPassThrough {
    /// The name of the component.
    pub const NAME: &'static str = "miden::standards::auth::pass_through";

    /// Returns the canonical [`AccountComponentName`] of this component.
    pub const fn name() -> AccountComponentName {
        AccountComponentName::from_static_str(Self::NAME)
    }

    /// Returns the [`AccountComponentCode`] of this component.
    pub fn code() -> &'static AccountComponentCode {
        &AUTH_PASS_THROUGH_CODE
    }

    /// Creates a new [`AuthPassThrough`] component with the given approver.
    pub fn new(approver: Approver) -> Self {
        Self { approver }
    }

    /// Creates a new [`AuthPassThrough`] component using the Falcon512Poseidon2 signature scheme.
    ///
    /// The public key commitment is derived from the provided Falcon512 public key.
    pub fn falcon512_poseidon2(pub_key: falcon512_poseidon2::PublicKey) -> Self {
        Self {
            approver: Approver::new(pub_key.into(), AuthScheme::Falcon512Poseidon2),
        }
    }

    /// Creates a new [`AuthPassThrough`] component using the EcdsaK256Keccak signature scheme.
    ///
    /// The public key commitment is derived from the provided ECDSA K256 public key.
    ///
    /// Note: this scheme discloses the signer's public key and signature at proving time and
    /// therefore does not provide public-key privacy. See
    /// [`AuthScheme::EcdsaK256Keccak`][scheme] for details, and prefer
    /// [`falcon512_poseidon2`](Self::falcon512_poseidon2) if signer-key privacy is required.
    ///
    /// [scheme]: miden_protocol::account::auth::AuthScheme::EcdsaK256Keccak
    pub fn ecdsa_k256_keccak(pub_key: ecdsa_k256_keccak::PublicKey) -> Self {
        Self {
            approver: Approver::new(pub_key.into(), AuthScheme::EcdsaK256Keccak),
        }
    }

    /// Creates a new [`AuthPassThrough`] component from a [`PublicKey`].
    ///
    /// The authentication scheme and public key commitment are derived from the provided key.
    pub fn from_public_key(pub_key: PublicKey) -> Self {
        Self {
            approver: Approver::new(pub_key.to_commitment(), pub_key.auth_scheme()),
        }
    }

    /// Returns the approver of this component.
    pub fn approver(&self) -> Approver {
        self.approver
    }

    /// Returns the [`StorageSlotName`] where the public key is stored.
    pub fn public_key_slot() -> &'static StorageSlotName {
        &PUBKEY_SLOT_NAME
    }

    /// Returns the [`StorageSlotName`] where the scheme ID is stored.
    pub fn scheme_id_slot() -> &'static StorageSlotName {
        &SCHEME_ID_SLOT_NAME
    }

    /// Returns the storage slot schema for the public key slot.
    pub fn public_key_slot_schema() -> (StorageSlotName, StorageSlotSchema) {
        (
            Self::public_key_slot().clone(),
            StorageSlotSchema::value("Public key commitment", SchemaType::pub_key()),
        )
    }

    /// Returns the storage slot schema for the scheme ID slot.
    pub fn auth_scheme_slot_schema() -> (StorageSlotName, StorageSlotSchema) {
        (
            Self::scheme_id_slot().clone(),
            StorageSlotSchema::value("Scheme ID", SchemaType::auth_scheme()),
        )
    }

    /// Returns the [`AccountComponentMetadata`] for this component.
    pub fn component_metadata() -> AccountComponentMetadata {
        let storage_schema = StorageSchema::new(vec![
            Self::public_key_slot_schema(),
            Self::auth_scheme_slot_schema(),
        ])
        .expect("storage schema should be valid");

        AccountComponentMetadata::new(Self::NAME)
            .with_description("Pass-through authentication component")
            .with_storage_schema(storage_schema)
    }
}

impl From<AuthPassThrough> for AccountComponent {
    fn from(pass_through: AuthPassThrough) -> Self {
        let metadata = AuthPassThrough::component_metadata();

        let storage_slots = vec![
            StorageSlot::with_value(
                AuthPassThrough::public_key_slot().clone(),
                pass_through.approver.pub_key().into(),
            ),
            StorageSlot::with_value(
                AuthPassThrough::scheme_id_slot().clone(),
                Word::from([pass_through.approver.auth_scheme().as_u8(), 0, 0, 0]),
            ),
        ];

        AccountComponent::new(AuthPassThrough::code().clone(), storage_slots, metadata).expect(
            "AuthPassThrough component should satisfy the requirements of a valid account \
             component",
        )
    }
}

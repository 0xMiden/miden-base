use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use miden_objects::account::auth::PublicKeyCommitment;
use miden_objects::account::{AccountId, AccountProcedureInfo, AccountStorage, StorageSlotName};
use miden_objects::note::PartialNote;
use miden_objects::{Felt, FieldElement, Word};

use crate::AuthScheme;
use crate::account::auth::{
    AuthEcdsaK256Keccak,
    AuthEcdsaK256KeccakAcl,
    AuthEcdsaK256KeccakMultisig,
    AuthRpoFalcon512,
    AuthRpoFalcon512Acl,
    AuthRpoFalcon512Multisig,
};
use crate::account::components::WellKnownComponent;
use crate::account::interface::AccountInterfaceError;

// ACCOUNT COMPONENT INTERFACE
// ================================================================================================

/// The enum holding all possible account interfaces which could be loaded to some account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountComponentInterface {
    /// Exposes procedures from the [`BasicWallet`][crate::account::wallets::BasicWallet] module.
    BasicWallet,
    /// Exposes procedures from the
    /// [`BasicFungibleFaucet`][crate::account::faucets::BasicFungibleFaucet] module.
    BasicFungibleFaucet,
    /// Exposes procedures from the
    /// [`NetworkFungibleFaucet`][crate::account::faucets::NetworkFungibleFaucet] module.
    NetworkFungibleFaucet,
    /// Exposes procedures from the
    /// [`AuthEcdsaK256Keccak`][crate::account::auth::AuthEcdsaK256Keccak] module.
    AuthEcdsaK256Keccak,
    /// Exposes procedures from the
    /// [`AuthEcdsaK256KeccakAcl`][crate::account::auth::AuthEcdsaK256KeccakAcl] module.
    AuthEcdsaK256KeccakAcl,
    /// Exposes procedures from the
    /// [`AuthEcdsaK256KeccakMultisig`][crate::account::auth::AuthEcdsaK256KeccakMultisig] module.
    AuthEcdsaK256KeccakMultisig,
    /// Exposes procedures from the
    /// [`AuthRpoFalcon512`][crate::account::auth::AuthRpoFalcon512] module.
    AuthRpoFalcon512,
    /// Exposes procedures from the
    /// [`AuthRpoFalcon512Acl`][crate::account::auth::AuthRpoFalcon512Acl] module.
    AuthRpoFalcon512Acl,
    /// Exposes procedures from the
    /// [`AuthRpoFalcon512Multisig`][crate::account::auth::AuthRpoFalcon512Multisig] module.
    AuthRpoFalcon512Multisig,
    /// Exposes procedures from the [`NoAuth`][crate::account::auth::NoAuth] module.
    ///
    /// This authentication scheme provides no cryptographic authentication and only increments
    /// the nonce if the account state has actually changed during transaction execution.
    AuthNoAuth,
    /// A non-standard, custom interface which exposes the contained procedures.
    ///
    /// Custom interface holds procedures which are not part of some standard interface which is
    /// used by this account. Each custom interface holds procedures with the same storage offset.
    Custom(Vec<AccountProcedureInfo>),
}

impl AccountComponentInterface {
    /// Returns a string line with the name of the [AccountComponentInterface] enum variant.
    ///
    /// In case of a [AccountComponentInterface::Custom] along with the name of the enum variant
    /// the vector of shortened hex representations of the used procedures is returned, e.g.
    /// `Custom([0x6d93447, 0x0bf23d8])`.
    pub fn name(&self) -> String {
        match self {
            AccountComponentInterface::BasicWallet => "Basic Wallet".to_string(),
            AccountComponentInterface::BasicFungibleFaucet => "Basic Fungible Faucet".to_string(),
            AccountComponentInterface::NetworkFungibleFaucet => {
                "Network Fungible Faucet".to_string()
            },
            AccountComponentInterface::AuthEcdsaK256Keccak => "ECDSA K256 Keccak".to_string(),
            AccountComponentInterface::AuthEcdsaK256KeccakAcl => {
                "ECDSA K256 Keccak ACL".to_string()
            },
            AccountComponentInterface::AuthEcdsaK256KeccakMultisig => {
                "ECDSA K256 Keccak Multisig".to_string()
            },
            AccountComponentInterface::AuthRpoFalcon512 => "RPO Falcon512".to_string(),
            AccountComponentInterface::AuthRpoFalcon512Acl => "RPO Falcon512 ACL".to_string(),
            AccountComponentInterface::AuthRpoFalcon512Multisig => {
                "RPO Falcon512 Multisig".to_string()
            },

            AccountComponentInterface::AuthNoAuth => "No Auth".to_string(),
            AccountComponentInterface::Custom(proc_info_vec) => {
                let result = proc_info_vec
                    .iter()
                    .map(|proc_info| proc_info.mast_root().to_hex()[..9].to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Custom([{result}])")
            },
        }
    }

    /// Returns true if this component interface is an authentication component.
    ///
    /// TODO: currently this can identify only standard auth components
    pub fn is_auth_component(&self) -> bool {
        matches!(
            self,
            AccountComponentInterface::AuthEcdsaK256Keccak
                | AccountComponentInterface::AuthEcdsaK256KeccakAcl
                | AccountComponentInterface::AuthEcdsaK256KeccakMultisig
                | AccountComponentInterface::AuthRpoFalcon512
                | AccountComponentInterface::AuthRpoFalcon512Acl
                | AccountComponentInterface::AuthRpoFalcon512Multisig
                | AccountComponentInterface::AuthNoAuth
        )
    }

    /// Returns the authentication schemes associated with this component interface.
    pub fn get_auth_schemes(&self, storage: &AccountStorage) -> Vec<AuthScheme> {
        match self {
            AccountComponentInterface::AuthEcdsaK256Keccak => {
                vec![AuthScheme::EcdsaK256Keccak {
                    pub_key: PublicKeyCommitment::from(
                        storage
                            .get_item(AuthEcdsaK256Keccak::public_key_slot())
                            .expect("invalid storage index of the public key"),
                    ),
                }]
            },
            AccountComponentInterface::AuthEcdsaK256KeccakAcl => {
                vec![AuthScheme::EcdsaK256Keccak {
                    pub_key: PublicKeyCommitment::from(
                        storage
                            .get_item(AuthEcdsaK256KeccakAcl::public_key_slot())
                            .expect("invalid storage index of the public key"),
                    ),
                }]
            },
            AccountComponentInterface::AuthEcdsaK256KeccakMultisig => {
                vec![extract_multisig_auth_scheme(
                    storage,
                    AuthEcdsaK256KeccakMultisig::threshold_config_slot(),
                    AuthEcdsaK256KeccakMultisig::approver_public_keys_slot(),
                )]
            },
            AccountComponentInterface::AuthRpoFalcon512 => {
                vec![AuthScheme::RpoFalcon512 {
                    pub_key: PublicKeyCommitment::from(
                        storage
                            .get_item(AuthRpoFalcon512::public_key_slot())
                            .expect("invalid slot name of the AuthRpoFalcon512 public key"),
                    ),
                }]
            },
            AccountComponentInterface::AuthRpoFalcon512Acl => {
                vec![AuthScheme::RpoFalcon512 {
                    pub_key: PublicKeyCommitment::from(
                        storage
                            .get_item(AuthRpoFalcon512Acl::public_key_slot())
                            .expect("invalid slot name of the AuthRpoFalcon512Acl public key"),
                    ),
                }]
            },
            AccountComponentInterface::AuthRpoFalcon512Multisig => {
                vec![extract_multisig_auth_scheme(
                    storage,
                    AuthRpoFalcon512Multisig::threshold_config_slot(),
                    AuthRpoFalcon512Multisig::approver_public_keys_slot(),
                )]
            },
            AccountComponentInterface::AuthNoAuth => vec![AuthScheme::NoAuth],
            _ => vec![], // Non-auth components return empty vector
        }
    }

    /// Creates a vector of [AccountComponentInterface] instances. This vector specifies the
    /// components which were used to create an account with the provided procedures info array.
    pub fn from_procedures(procedures: &[AccountProcedureInfo]) -> Vec<Self> {
        let mut component_interface_vec = Vec::new();

        let mut procedures: BTreeMap<_, _> = procedures
            .iter()
            .map(|procedure_info| (*procedure_info.mast_root(), procedure_info))
            .collect();

        // Well known component interfaces
        // ----------------------------------------------------------------------------------------

        // Get all available well known components which could be constructed from the `procedures`
        // map and push them to the `component_interface_vec`
        WellKnownComponent::extract_well_known_components(
            &mut procedures,
            &mut component_interface_vec,
        );

        // Custom component interfaces
        // ----------------------------------------------------------------------------------------

        // All remaining procedures are put into the custom bucket.
        component_interface_vec
            .push(AccountComponentInterface::Custom(procedures.into_values().copied().collect()));

        component_interface_vec
    }

    /// Generates a body for the note creation of the `send_note` transaction script. The resulting
    /// code could use different procedures for note creation, which depends on the used interface.
    ///
    /// The body consists of two sections:
    /// - Pushing the note information on the stack.
    /// - Creating a note:
    ///   - For basic fungible faucet: pushing the amount of assets and distributing them.
    ///   - For basic wallet: creating a note, pushing the assets on the stack and moving them to
    ///     the created note.
    ///
    /// # Examples
    ///
    /// Example script for the [`AccountComponentInterface::BasicWallet`] with one note:
    ///
    /// ```masm
    ///     push.{note_information}
    ///     call.::miden::output_note::create
    ///
    ///     push.{note asset}
    ///     call.::miden::contracts::wallets::basic::move_asset_to_note dropw
    ///     dropw dropw dropw drop
    /// ```
    ///
    /// Example script for the [`AccountComponentInterface::BasicFungibleFaucet`] with one note:
    ///
    /// ```masm
    ///     push.{note information}
    ///
    ///     push.{asset amount}
    ///     call.::miden::contracts::faucets::basic_fungible::distribute dropw dropw drop
    /// ```
    ///
    /// # Errors:
    /// Returns an error if:
    /// - the interface does not support the generation of the standard `send_note` procedure.
    /// - the sender of the note isn't the account for which the script is being built.
    /// - the note created by the faucet doesn't contain exactly one asset.
    /// - a faucet tries to distribute an asset with a different faucet ID.
    pub(crate) fn send_note_body(
        &self,
        sender_account_id: AccountId,
        notes: &[PartialNote],
    ) -> Result<String, AccountInterfaceError> {
        let mut body = String::new();

        for partial_note in notes {
            if partial_note.metadata().sender() != sender_account_id {
                return Err(AccountInterfaceError::InvalidSenderAccount(
                    partial_note.metadata().sender(),
                ));
            }

            body.push_str(&format!(
                "push.{recipient}
                push.{execution_hint}
                push.{note_type}
                push.{aux}
                push.{tag}\n",
                recipient = partial_note.recipient_digest(),
                note_type = Felt::from(partial_note.metadata().note_type()),
                execution_hint = Felt::from(partial_note.metadata().execution_hint()),
                aux = partial_note.metadata().aux(),
                tag = Felt::from(partial_note.metadata().tag()),
            ));
            // stack => [tag, aux, note_type, execution_hint, RECIPIENT]

            match self {
                AccountComponentInterface::BasicFungibleFaucet => {
                    if partial_note.assets().num_assets() != 1 {
                        return Err(AccountInterfaceError::FaucetNoteWithoutAsset);
                    }

                    // SAFETY: We checked that the note contains exactly one asset
                    let asset =
                        partial_note.assets().iter().next().expect("note should contain an asset");

                    if asset.faucet_id_prefix() != sender_account_id.prefix() {
                        return Err(AccountInterfaceError::IssuanceFaucetMismatch(
                            asset.faucet_id_prefix(),
                        ));
                    }

                    body.push_str(&format!(
                        "push.{amount}
                        call.::miden::contracts::faucets::basic_fungible::distribute dropw dropw drop\n",
                        amount = asset.unwrap_fungible().amount()
                    ));
                    // stack => []
                },
                AccountComponentInterface::BasicWallet => {
                    body.push_str("call.::miden::output_note::create\n");
                    // stack => [note_idx]

                    for asset in partial_note.assets().iter() {
                        body.push_str(&format!(
                            "push.{asset}
                            call.::miden::contracts::wallets::basic::move_asset_to_note dropw\n",
                            asset = Word::from(*asset)
                        ));
                        // stack => [note_idx]
                    }

                    body.push_str("dropw dropw dropw drop\n");
                    // stack => []
                },
                _ => {
                    return Err(AccountInterfaceError::UnsupportedInterface {
                        interface: self.clone(),
                    });
                },
            }
        }

        Ok(body)
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Extracts authentication scheme from a multisig component.
fn extract_multisig_auth_scheme(
    storage: &AccountStorage,
    config_slot: &StorageSlotName,
    approver_public_keys_slot: &StorageSlotName,
) -> AuthScheme {
    // Read the multisig configuration from the config slot
    // Format: [threshold, num_approvers, 0, 0]
    let config = storage
        .get_item(config_slot)
        .expect("invalid slot name of the multisig configuration");

    let threshold = config[0].as_int() as u32;
    let num_approvers = config[1].as_int() as u8;

    let mut pub_keys = Vec::new();

    // Read each public key from the map
    for key_index in 0..num_approvers {
        // The multisig component stores keys using pattern [index, 0, 0, 0]
        let map_key = [Felt::new(key_index as u64), Felt::ZERO, Felt::ZERO, Felt::ZERO];

        match storage.get_map_item(approver_public_keys_slot, map_key.into()) {
            Ok(pub_key) => {
                pub_keys.push(PublicKeyCommitment::from(pub_key));
            },
            Err(_) => {
                // If we can't read a public key, panic with a clear error message
                panic!(
                    "Failed to read public key {} from multisig configuration at storage slot {}. \
                        Expected key pattern [index, 0, 0, 0]. \
                        This indicates corrupted multisig storage or incorrect storage layout.",
                    key_index, approver_public_keys_slot
                );
            },
        }
    }

    AuthScheme::RpoFalcon512Multisig { threshold, pub_keys }
}

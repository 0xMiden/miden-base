use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use miden_objects::account::{AccountId, AccountProcedureInfo};
use miden_objects::crypto::dsa::rpo_falcon512::PublicKey;
use miden_objects::note::PartialNote;
use miden_objects::{Felt, Word};

use crate::AuthScheme;
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
    ///
    /// Internal value holds the storage slot index where faucet metadata is stored. This metadata
    /// slot has a format of `[max_supply, faucet_decimals, token_symbol, 0]`.
    BasicFungibleFaucet(u8),
    /// Exposes procedures from the
    /// [`AuthRpoFalcon512`][crate::account::auth::AuthRpoFalcon512] module.
    ///
    /// Internal value holds the storage slot index where the public key for the RpoFalcon512
    /// authentication scheme is stored.
    AuthRpoFalcon512(u8),
    /// Exposes procedures from the
    /// [`AuthRpoFalcon512Acl`][crate::account::auth::AuthRpoFalcon512Acl] module.
    ///
    /// Internal value holds the storage slot index where the public key for the RpoFalcon512
    /// authentication scheme is stored.
    AuthRpoFalcon512Acl(u8),
    /// Exposes procedures from the multisig RpoFalcon512 authentication module.
    ///
    /// Internal value holds the storage slot index where the multisig configuration is stored.
    AuthRpoFalcon512Multisig(u8),
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
            AccountComponentInterface::BasicFungibleFaucet(_) => {
                "Basic Fungible Faucet".to_string()
            },
            AccountComponentInterface::AuthRpoFalcon512(_) => "RPO Falcon512".to_string(),
            AccountComponentInterface::AuthRpoFalcon512Acl(_) => "RPO Falcon512 ACL".to_string(),
            AccountComponentInterface::AuthRpoFalcon512Multisig(_) => {
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
    pub fn is_auth_component(&self) -> bool {
        matches!(
            self,
            AccountComponentInterface::AuthRpoFalcon512(_)
                | AccountComponentInterface::AuthRpoFalcon512Acl(_)
                | AccountComponentInterface::AuthRpoFalcon512Multisig(_)
                | AccountComponentInterface::AuthNoAuth
        )
    }

    /// Returns the authentication schemes associated with this component interface.
    ///
    /// This method extracts all authentication schemes from the component interface by examining
    /// the account storage at the appropriate storage indices for authentication components.
    ///
    /// # Arguments
    /// * `storage` - The account storage to read authentication data from
    ///
    /// # Returns
    /// * `Vec<AuthScheme>` - Vector of authentication schemes for this component
    ///
    /// # Limitations
    /// Currently, this method only detects known authentication schemes. For custom authentication
    /// components, it would return an empty vector even if they are authentication components.
    ///
    /// # Future Improvements
    /// A more generic approach could be implemented where:
    /// - `from_procedures` returns `(Vec<Self>, Word)` with the auth procedure MAST root
    /// - Callers pass a generic `T: AccountAuthComponent where AccountAuthComponent:
    ///   TryFrom<&AccountStorage>`
    /// - This would allow detection and extraction of custom auth components without knowing their
    ///   layout
    pub fn get_auth_schemes(
        &self,
        storage: &miden_objects::account::AccountStorage,
    ) -> Vec<AuthScheme> {
        match self {
            AccountComponentInterface::AuthRpoFalcon512(storage_index)
            | AccountComponentInterface::AuthRpoFalcon512Acl(storage_index) => {
                vec![AuthScheme::RpoFalcon512 {
                    pub_key: PublicKey::new(
                        storage
                            .get_item(*storage_index)
                            .expect("invalid storage index of the public key"),
                    ),
                }]
            },
            AccountComponentInterface::AuthRpoFalcon512Multisig(storage_index) => {
                // TODO: Implement proper multisig auth scheme extraction
                // For now, we need to determine how to extract multisig configuration from storage
                // This might require reading multiple storage slots for the multisig setup
                // In the future, this could return multiple AuthSchemes for different signers
                vec![AuthScheme::RpoFalcon512 {
                    pub_key: PublicKey::new(
                        storage
                            .get_item(*storage_index)
                            .expect("invalid storage index of the multisig configuration"),
                    ),
                }]
            },
            AccountComponentInterface::AuthNoAuth => vec![AuthScheme::NoAuth],
            _ => vec![],
        }
    }

    /// Returns the authentication scheme associated with this component interface, if any.
    ///
    /// This method extracts the authentication scheme from the component interface by examining
    /// the account storage at the appropriate storage index for authentication components.
    ///
    /// # Arguments
    /// * `storage` - The account storage to read authentication data from
    ///
    /// # Returns
    /// * `Some(AuthScheme)` - If this is an authentication component interface
    /// * `None` - If this is not an authentication component interface
    ///
    /// # Deprecated
    /// This method is deprecated in favor of `get_auth_schemes()` which can return multiple
    /// authentication schemes from a single component.
    #[deprecated(since = "0.11.0", note = "Use get_auth_schemes() instead")]
    pub fn get_auth_scheme(
        &self,
        storage: &miden_objects::account::AccountStorage,
    ) -> Option<AuthScheme> {
        self.get_auth_schemes(storage).into_iter().next()
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

        let mut custom_interface_procs_map = BTreeMap::<u8, Vec<AccountProcedureInfo>>::new();
        procedures.into_iter().for_each(|(_, proc_info)| {
            match custom_interface_procs_map.get_mut(&proc_info.storage_offset()) {
                Some(proc_vec) => proc_vec.push(*proc_info),
                None => {
                    custom_interface_procs_map.insert(proc_info.storage_offset(), vec![*proc_info]);
                },
            }
        });

        if !custom_interface_procs_map.is_empty() {
            for proc_vec in custom_interface_procs_map.into_values() {
                component_interface_vec.push(AccountComponentInterface::Custom(proc_vec));
            }
        }

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
    ///     call.::miden::tx::create_note
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
                AccountComponentInterface::BasicFungibleFaucet(_) => {
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
                    body.push_str("call.::miden::tx::create_note\n");
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

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use miden_objects::Felt;
use miden_objects::account::{AccountId, AccountProcedureInfo};
use miden_objects::note::PartialNote;
use miden_objects::utils::word_to_masm_push_string;

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
            AccountComponentInterface::AuthRpoFalcon512Acl(_) => {
                "RPO Falcon512 ACL".to_string()
            },
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
                recipient = word_to_masm_push_string(&partial_note.recipient_digest()),
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
                            asset = word_to_masm_push_string(&asset.into())
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

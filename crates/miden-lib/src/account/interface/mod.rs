use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use miden_objects::Word;
use miden_objects::account::{Account, AccountCode, AccountId, AccountIdPrefix, AccountType};
use miden_objects::assembly::mast::{MastForest, MastNode, MastNodeId};
use miden_objects::crypto::dsa::rpo_falcon512;
use miden_objects::note::{Note, NoteScript, PartialNote};
use miden_objects::transaction::TransactionScript;
use thiserror::Error;

use crate::AuthScheme;
use crate::account::components::{
    basic_fungible_faucet_library,
    basic_wallet_library,
    rpo_falcon_512_library,
    rpo_falcon_512_acl_library,
};
use crate::errors::ScriptBuilderError;
use crate::note::well_known_note::WellKnownNote;
use crate::utils::ScriptBuilder;

#[cfg(test)]
mod test;

mod component;
pub use component::AccountComponentInterface;

// ACCOUNT INTERFACE
// ================================================================================================

/// An [`AccountInterface`] describes the exported, callable procedures of an account.
///
/// A note script's compatibility with this interface can be inspected to check whether the note may
/// result in a successful execution against this account.
pub struct AccountInterface {
    account_id: AccountId,
    auth: Vec<AuthScheme>,
    components: Vec<AccountComponentInterface>,
}

// ------------------------------------------------------------------------------------------------
/// Constructors and public accessors
impl AccountInterface {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`AccountInterface`] instance from the provided account ID, authentication
    /// schemes and account code.
    pub fn new(account_id: AccountId, auth: Vec<AuthScheme>, code: &AccountCode) -> Self {
        let components = AccountComponentInterface::from_procedures(code.procedures());

        Self { account_id, auth, components }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the account ID.
    pub fn id(&self) -> &AccountId {
        &self.account_id
    }

    /// Returns the type of the reference account.
    pub fn account_type(&self) -> AccountType {
        self.account_id.account_type()
    }

    /// Returns true if the reference account can issue assets.
    pub fn is_faucet(&self) -> bool {
        self.account_id.is_faucet()
    }

    /// Returns true if the reference account is a regular.
    pub fn is_regular_account(&self) -> bool {
        self.account_id.is_regular_account()
    }

    /// Returns `true` if the full state of the account is on chain, i.e. if the modes are
    /// [`AccountStorageMode::Public`](miden_objects::account::AccountStorageMode::Public) or
    /// [`AccountStorageMode::Network`](miden_objects::account::AccountStorageMode::Network),
    /// `false` otherwise.
    pub fn is_onchain(&self) -> bool {
        self.account_id.is_onchain()
    }

    /// Returns `true` if the reference account is a private account, `false` otherwise.
    pub fn is_private(&self) -> bool {
        self.account_id.is_private()
    }

    /// Returns true if the reference account is a public account, `false` otherwise.
    pub fn is_public(&self) -> bool {
        self.account_id.is_public()
    }

    /// Returns true if the reference account is a network account, `false` otherwise.
    pub fn is_network(&self) -> bool {
        self.account_id.is_network()
    }

    /// Returns a reference to the vector of used authentication schemes.
    pub fn auth(&self) -> &Vec<AuthScheme> {
        &self.auth
    }

    /// Returns a reference to the set of used component interfaces.
    pub fn components(&self) -> &Vec<AccountComponentInterface> {
        &self.components
    }

    /// Returns [NoteAccountCompatibility::Maybe] if the provided note is compatible with the
    /// current [AccountInterface], and [NoteAccountCompatibility::No] otherwise.
    pub fn is_compatible_with(&self, note: &Note) -> NoteAccountCompatibility {
        if let Some(well_known_note) = WellKnownNote::from_note(note) {
            if well_known_note.is_compatible_with(self) {
                NoteAccountCompatibility::Maybe
            } else {
                NoteAccountCompatibility::No
            }
        } else {
            verify_note_script_compatibility(note.script(), self.get_procedure_digests())
        }
    }

    /// Returns a digests set of all procedures from all account component interfaces.
    pub(crate) fn get_procedure_digests(&self) -> BTreeSet<Word> {
        let mut component_proc_digests = BTreeSet::new();
        for component in self.components.iter() {
            match component {
                AccountComponentInterface::BasicWallet => {
                    component_proc_digests
                        .extend(basic_wallet_library().mast_forest().procedure_digests());
                },
                AccountComponentInterface::BasicFungibleFaucet(_) => {
                    component_proc_digests
                        .extend(basic_fungible_faucet_library().mast_forest().procedure_digests());
                },
                AccountComponentInterface::AuthRpoFalcon512(_) => {
                    component_proc_digests
                        .extend(rpo_falcon_512_library().mast_forest().procedure_digests());
                },
                AccountComponentInterface::AuthRpoFalcon512Acl(_) => {
                    component_proc_digests.extend(
                        rpo_falcon_512_acl_library().mast_forest().procedure_digests(),
                    );
                },
                AccountComponentInterface::Custom(custom_procs) => {
                    component_proc_digests
                        .extend(custom_procs.iter().map(|info| *info.mast_root()));
                },
            }
        }

        component_proc_digests
    }
}

// ------------------------------------------------------------------------------------------------
/// Code generation
impl AccountInterface {
    /// Returns a transaction script which sends the specified notes using the procedures available
    /// in the current interface.
    ///
    /// Provided `expiration_delta` parameter is used to specify how close to the transaction's
    /// reference block the transaction must be included into the chain. For example, if the
    /// transaction's reference block is 100 and transaction expiration delta is 10, the transaction
    /// can be included into the chain by block 110. If this does not happen, the transaction is
    /// considered expired and cannot be included into the chain.
    ///
    /// Currently only [`AccountComponentInterface::BasicWallet`] and
    /// [`AccountComponentInterface::BasicFungibleFaucet`] interfaces are supported for the
    /// `send_note` script creation. Attempt to generate the script using some other interface will
    /// lead to an error. In case both supported interfaces are available in the account, the script
    /// will be generated for the [`AccountComponentInterface::BasicFungibleFaucet`] interface.
    ///
    /// # Example
    ///
    /// Example of the `send_note` script with specified expiration delta and one output note:
    ///
    /// ```masm
    /// begin
    ///     push.{expiration_delta} exec.::miden::tx::update_expiration_block_delta
    ///
    ///     push.{note information}
    ///
    ///     push.{asset amount}
    ///     call.::miden::contracts::faucets::basic_fungible::distribute dropw dropw drop
    /// end
    /// ```
    ///
    /// # Errors:
    /// Returns an error if:
    /// - the available interfaces does not support the generation of the standard `send_note`
    ///   procedure.
    /// - the sender of the note isn't the account for which the script is being built.
    /// - the note created by the faucet doesn't contain exactly one asset.
    /// - a faucet tries to distribute an asset with a different faucet ID.
    ///
    /// [wallet]: crate::account::interface::AccountComponentInterface::BasicWallet
    /// [faucet]: crate::account::interface::AccountComponentInterface::BasicFungibleFaucet
    pub fn build_send_notes_script(
        &self,
        output_notes: &[PartialNote],
        expiration_delta: Option<u16>,
        in_debug_mode: bool,
    ) -> Result<TransactionScript, AccountInterfaceError> {
        let note_creation_source = self.build_create_notes_section(output_notes)?;

        let script = format!(
            "begin\n{}\n{}\nend",
            self.build_set_tx_expiration_section(expiration_delta),
            note_creation_source,
        );

        let tx_script = ScriptBuilder::new(in_debug_mode)
            .compile_tx_script(script)
            .map_err(AccountInterfaceError::InvalidTransactionScript)?;

        Ok(tx_script)
    }

    /// Generates a note creation code required for the `send_note` transaction script.
    ///
    /// For the example of the resulting code see [AccountComponentInterface::send_note_body]
    /// description.
    ///
    /// # Errors:
    /// Returns an error if:
    /// - the available interfaces does not support the generation of the standard `send_note`
    ///   procedure.
    /// - the sender of the note isn't the account for which the script is being built.
    /// - the note created by the faucet doesn't contain exactly one asset.
    /// - a faucet tries to distribute an asset with a different faucet ID.
    fn build_create_notes_section(
        &self,
        output_notes: &[PartialNote],
    ) -> Result<String, AccountInterfaceError> {
        if let Some(basic_fungible_faucet) = self.components().iter().find(|component_interface| {
            matches!(component_interface, AccountComponentInterface::BasicFungibleFaucet(_))
        }) {
            basic_fungible_faucet.send_note_body(*self.id(), output_notes)
        } else if self.components().contains(&AccountComponentInterface::BasicWallet) {
            AccountComponentInterface::BasicWallet.send_note_body(*self.id(), output_notes)
        } else {
            Err(AccountInterfaceError::UnsupportedAccountInterface)
        }
    }

    /// Returns a string with the expiration delta update procedure call for the script.
    fn build_set_tx_expiration_section(&self, expiration_delta: Option<u16>) -> String {
        if let Some(expiration_delta) = expiration_delta {
            format!("push.{expiration_delta} exec.::miden::tx::update_expiration_block_delta\n")
        } else {
            String::new()
        }
    }
}

impl From<&Account> for AccountInterface {
    fn from(account: &Account) -> Self {
        let components = AccountComponentInterface::from_procedures(account.code().procedures());
        let mut auth = Vec::new();
        components.iter().for_each(|interface| {
            match interface {
                // RpoFalcon512 and RpoFalcon512Acl use the same RpoFalcon512 auth scheme
                AccountComponentInterface::AuthRpoFalcon512(storage_index)
                | AccountComponentInterface::AuthRpoFalcon512Acl(storage_index) => {
                    auth.push(AuthScheme::RpoFalcon512 {
                        pub_key: rpo_falcon512::PublicKey::new(
                            account
                                .storage()
                                .get_item(*storage_index)
                                .expect("invalid storage index of the public key"),
                        ),
                    })
                },
                _ => {},
            }
        });

        Self {
            account_id: account.id(),
            auth,
            components,
        }
    }
}

// NOTE ACCOUNT COMPATIBILITY
// ================================================================================================

/// Describes whether a note is compatible with a specific account.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteAccountCompatibility {
    /// A note is incompatible with an account.
    ///
    /// The account interface does not have procedures for being able to execute at least one of
    /// the program execution branches.
    No,
    /// The account has all necessary procedures of one execution branch of the note script. This
    /// means the note may be able to be consumed by the account if that branch is executed.
    Maybe,
    /// A note could be successfully executed and consumed by the account.
    Yes,
}

// HELPER FUNCTIONS
// ------------------------------------------------------------------------------------------------

/// Verifies that the provided note script is compatible with the target account interfaces.
///
/// This is achieved by checking that at least one execution branch in the note script is compatible
/// with the account procedures vector.
///
/// This check relies on the fact that account procedures are the only procedures that are `call`ed
/// from note scripts, while kernel procedures are `sycall`ed.
fn verify_note_script_compatibility(
    note_script: &NoteScript,
    account_procedures: BTreeSet<Word>,
) -> NoteAccountCompatibility {
    // collect call branches of the note script
    let branches = collect_call_branches(note_script);

    // if none of the branches are compatible with the target account, return a `CheckResult::No`
    if !branches.iter().any(|call_targets| call_targets.is_subset(&account_procedures)) {
        return NoteAccountCompatibility::No;
    }

    NoteAccountCompatibility::Maybe
}

/// Collect call branches by recursively traversing through program execution branches and
/// accumulating call targets.
fn collect_call_branches(note_script: &NoteScript) -> Vec<BTreeSet<Word>> {
    let mut branches = vec![BTreeSet::new()];

    let entry_node = note_script.entrypoint();
    recursively_collect_call_branches(entry_node, &mut branches, &note_script.mast());
    branches
}

/// Generates a list of calls invoked in each execution branch of the provided code block.
fn recursively_collect_call_branches(
    mast_node_id: MastNodeId,
    branches: &mut Vec<BTreeSet<Word>>,
    note_script_forest: &Arc<MastForest>,
) {
    let mast_node = &note_script_forest[mast_node_id];

    match mast_node {
        MastNode::Block(_) => {},
        MastNode::Join(join_node) => {
            recursively_collect_call_branches(join_node.first(), branches, note_script_forest);
            recursively_collect_call_branches(join_node.second(), branches, note_script_forest);
        },
        MastNode::Split(split_node) => {
            let current_branch = branches.last().expect("at least one execution branch").clone();
            recursively_collect_call_branches(split_node.on_false(), branches, note_script_forest);

            // If the previous branch had additional calls we need to create a new branch
            if branches.last().expect("at least one execution branch").len() > current_branch.len()
            {
                branches.push(current_branch);
            }

            recursively_collect_call_branches(split_node.on_true(), branches, note_script_forest);
        },
        MastNode::Loop(loop_node) => {
            recursively_collect_call_branches(loop_node.body(), branches, note_script_forest);
        },
        MastNode::Call(call_node) => {
            if call_node.is_syscall() {
                return;
            }

            let callee_digest = note_script_forest[call_node.callee()].digest();

            branches
                .last_mut()
                .expect("at least one execution branch")
                .insert(callee_digest);
        },
        MastNode::Dyn(_) => {},
        MastNode::External(_) => {},
    }
}

// ACCOUNT INTERFACE ERROR
// ============================================================================================

/// Account interface related errors.
#[derive(Debug, Error)]
pub enum AccountInterfaceError {
    #[error("note asset is not issued by this faucet: {0}")]
    IssuanceFaucetMismatch(AccountIdPrefix),
    #[error("note created by the basic fungible faucet doesn't contain exactly one asset")]
    FaucetNoteWithoutAsset,
    #[error("invalid transaction script")]
    InvalidTransactionScript(#[source] ScriptBuilderError),
    #[error("invalid sender account: {0}")]
    InvalidSenderAccount(AccountId),
    #[error("{} interface does not support the generation of the standard send_note script", interface.name())]
    UnsupportedInterface { interface: AccountComponentInterface },
    #[error(
        "account does not contain the basic fungible faucet or basic wallet interfaces which are needed to support the send_note script generation"
    )]
    UnsupportedAccountInterface,
}

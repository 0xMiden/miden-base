use alloc::collections::BTreeSet;
use alloc::vec::Vec;

use miden_protocol::account::AccountProcedureRoot;

use crate::account::access::{Authority, Ownable2Step, RoleBasedAccessControl};
use crate::account::auth::{
    AuthGuardedMultisig,
    AuthMultisig,
    AuthMultisigSmart,
    AuthNetworkAccount,
    AuthSingleSig,
    NoAuth,
};
use crate::account::faucets::FungibleFaucet;
use crate::account::inspection::CodeInspection;
use crate::account::interface::AccountComponentInterface;
use crate::account::note_creator::NoteCreator;
use crate::account::wallets::BasicWallet;

// STANDARD ACCOUNT COMPONENTS
// ================================================================================================

/// The enum holding the types of standard account components defined in the `miden-standards`
/// crate.
pub enum StandardAccountComponent {
    BasicWallet,
    NoteCreator,
    FungibleFaucet,
    CodeInspection,
    Authority,
    Ownable2Step,
    RoleBasedAccessControl,
    AuthSingleSig,
    AuthMultisig,
    AuthMultisigSmart,
    AuthGuardedMultisig,
    AuthNoAuth,
    AuthNetworkAccount,
}

impl StandardAccountComponent {
    /// All standard components, in the order in which they are matched against an account.
    ///
    /// `NoteCreator` must follow `BasicWallet`: its only procedure (`create_note`) is also
    /// exported by the basic wallet, so the full wallet must claim it first to avoid misdetection.
    const ALL: [Self; 13] = [
        Self::BasicWallet,
        Self::NoteCreator,
        Self::FungibleFaucet,
        Self::CodeInspection,
        Self::Authority,
        Self::RoleBasedAccessControl,
        Self::Ownable2Step,
        Self::AuthSingleSig,
        Self::AuthGuardedMultisig,
        Self::AuthMultisig,
        Self::AuthMultisigSmart,
        Self::AuthNoAuth,
        Self::AuthNetworkAccount,
    ];

    /// Returns the iterator over the [`AccountProcedureRoot`]s of all procedures exported from
    /// the component.
    pub fn procedure_roots(&self) -> impl Iterator<Item = AccountProcedureRoot> {
        let code = match self {
            Self::BasicWallet => BasicWallet::code(),
            Self::NoteCreator => NoteCreator::code(),
            Self::FungibleFaucet => FungibleFaucet::code(),
            Self::CodeInspection => CodeInspection::code(),
            Self::Authority => Authority::code(),
            Self::Ownable2Step => Ownable2Step::code(),
            Self::RoleBasedAccessControl => RoleBasedAccessControl::code(),
            Self::AuthSingleSig => AuthSingleSig::code(),
            Self::AuthMultisig => AuthMultisig::code(),
            Self::AuthMultisigSmart => AuthMultisigSmart::code(),
            Self::AuthGuardedMultisig => AuthGuardedMultisig::code(),
            Self::AuthNoAuth => NoAuth::code(),
            Self::AuthNetworkAccount => AuthNetworkAccount::code(),
        };

        code.procedure_roots()
    }

    /// Checks whether procedures from the current component are present in `all_procedures` and if
    /// so it claims these procedures from `unclaimed_set` and pushes the corresponding component
    /// interface to the component interface vector.
    ///
    /// Presence is checked against `all_procedures` rather than against `unclaimed_set`, so a
    /// procedure root exported by two standard components (e.g. `has_procedure`, re-exported by
    /// both the fungible faucet and the code inspection component) satisfies both instead of being
    /// consumed by whichever component happens to be matched first. A component whose procedures
    /// were all claimed already is skipped, which keeps a narrower component from being reported
    /// alongside the broader one that contains it.
    ///
    /// TODO: replace with per-component detection once the `AccountComponentInterface` trait
    /// (issue #2621) lands.
    fn extract_component(
        &self,
        all_procedures: &BTreeSet<AccountProcedureRoot>,
        unclaimed_set: &mut BTreeSet<AccountProcedureRoot>,
        component_interface_vec: &mut Vec<AccountComponentInterface>,
    ) {
        // Determine if this component should be extracted based on procedure matching
        let is_exported = self.procedure_roots().all(|root| all_procedures.contains(&root));
        let is_unclaimed = self.procedure_roots().any(|root| unclaimed_set.contains(&root));

        if is_exported && is_unclaimed {
            // Claim the procedure root of any matching procedure.
            self.procedure_roots().for_each(|component_procedure| {
                unclaimed_set.remove(&component_procedure);
            });

            // Create the appropriate component interface
            match self {
                Self::BasicWallet => {
                    component_interface_vec.push(AccountComponentInterface::BasicWallet)
                },
                Self::NoteCreator => {
                    component_interface_vec.push(AccountComponentInterface::NoteCreator)
                },
                Self::FungibleFaucet => {
                    component_interface_vec.push(AccountComponentInterface::FungibleFaucet)
                },
                Self::CodeInspection => {
                    component_interface_vec.push(AccountComponentInterface::CodeInspection)
                },
                Self::Authority => {
                    component_interface_vec.push(AccountComponentInterface::Authority)
                },
                Self::Ownable2Step => {
                    component_interface_vec.push(AccountComponentInterface::Ownable2Step)
                },
                Self::RoleBasedAccessControl => {
                    component_interface_vec.push(AccountComponentInterface::RoleBasedAccessControl)
                },
                Self::AuthSingleSig => {
                    component_interface_vec.push(AccountComponentInterface::AuthSingleSig)
                },
                Self::AuthMultisig => {
                    component_interface_vec.push(AccountComponentInterface::AuthMultisig)
                },
                Self::AuthMultisigSmart => {
                    component_interface_vec.push(AccountComponentInterface::AuthMultisigSmart)
                },
                Self::AuthGuardedMultisig => {
                    component_interface_vec.push(AccountComponentInterface::AuthGuardedMultisig)
                },
                Self::AuthNoAuth => {
                    component_interface_vec.push(AccountComponentInterface::AuthNoAuth)
                },
                Self::AuthNetworkAccount => {
                    component_interface_vec.push(AccountComponentInterface::AuthNetworkAccount)
                },
            }
        }
    }

    /// Gets all standard components which could be constructed from the provided procedures map
    /// and pushes them to the `component_interface_vec`.
    ///
    /// On return, `procedures_set` holds exactly the procedures that no standard component
    /// claimed.
    pub fn extract_standard_components(
        procedures_set: &mut BTreeSet<AccountProcedureRoot>,
        component_interface_vec: &mut Vec<AccountComponentInterface>,
    ) {
        // Match against a snapshot of the full interface so that a procedure root exported by two
        // standard components can satisfy both, while `procedures_set` tracks what is still
        // unclaimed.
        let all_procedures = procedures_set.clone();

        for component in Self::ALL {
            component.extract_component(&all_procedures, procedures_set, component_interface_vec);
        }
    }
}

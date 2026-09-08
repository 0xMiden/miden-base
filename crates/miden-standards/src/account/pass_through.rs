use miden_protocol::account::component::{AccountComponentCode, AccountComponentMetadata};
use miden_protocol::account::{AccountComponent, AccountComponentName, AccountProcedureRoot};

use crate::account::account_component_code;
use crate::procedure_root;

// PASS THROUGH SWEEP
// ================================================================================================

account_component_code!(PASS_THROUGH_SWEEP_CODE, "miden-standards-pass-through-sweep.masp");

// PROCEDURE ROOTS
// ================================================================================================

/// MASL library namespace used for procedure-root lookups. Distinct from
/// [`PassThroughSweep::NAME`], which mirrors the standards-side MASM module path.
const PASS_THROUGH_SWEEP_LIBRARY_PATH: &str = "miden::standards::components::pass_through::sweep";

// Initialize the procedure root of the `sweep_asset_to_note` procedure only once.
procedure_root!(
    PASS_THROUGH_SWEEP_ASSET_TO_NOTE,
    PASS_THROUGH_SWEEP_LIBRARY_PATH,
    PassThroughSweep::SWEEP_ASSET_TO_NOTE_PROC_NAME,
    PassThroughSweep::code()
);

/// An [`AccountComponent`] providing the account procedure a pass-through transaction needs:
/// `sweep_asset_to_note`, which moves the account's entire balance of an asset into an output
/// note.
///
/// Unlike [`BasicWallet`](crate::account::wallets::BasicWallet)'s `move_asset_to_note`, which takes
/// the amount to move, this procedure reads the balance itself.
///
/// # Security
///
/// By itself, this procedure does not restrict who can invoke it. What bounds it is the account it
/// is installed on. For authenticated usage, pair it with
/// [`AuthPassThrough`](crate::account::auth::AuthPassThrough) so that only the holder of the
/// pubkey stored in the account can execute a transaction against it.
///
/// It is an account procedure, so the component must also be combined with one exposing
/// `receive_asset` (e.g. [`BasicWallet`](crate::account::wallets::BasicWallet)) so that input notes
/// can deposit into the account in the first place.
pub struct PassThroughSweep;

impl PassThroughSweep {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The name of the component.
    pub const NAME: &'static str = "miden::standards::pass_through::sweep";

    const SWEEP_ASSET_TO_NOTE_PROC_NAME: &str = "sweep_asset_to_note";

    /// Returns the canonical [`AccountComponentName`] of this component.
    pub const fn name() -> AccountComponentName {
        AccountComponentName::from_static_str(Self::NAME)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`AccountComponentCode`] of this component.
    pub fn code() -> &'static AccountComponentCode {
        &PASS_THROUGH_SWEEP_CODE
    }

    /// Returns the procedure root of the `sweep_asset_to_note` procedure.
    pub fn sweep_asset_to_note_root() -> AccountProcedureRoot {
        *PASS_THROUGH_SWEEP_ASSET_TO_NOTE
    }

    /// Returns the [`AccountComponentMetadata`] for this component.
    pub fn component_metadata() -> AccountComponentMetadata {
        AccountComponentMetadata::new(Self::NAME)
            .with_description("Pass-through component moving a whole account balance into a note")
    }
}

impl From<PassThroughSweep> for AccountComponent {
    fn from(_: PassThroughSweep) -> Self {
        let metadata = PassThroughSweep::component_metadata();

        AccountComponent::new(PassThroughSweep::code().clone(), vec![], metadata).expect(
            "pass through component should satisfy the requirements of a valid account component",
        )
    }
}

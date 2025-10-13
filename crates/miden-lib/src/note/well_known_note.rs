use alloc::string::{String, ToString};

use miden_objects::account::AccountId;
use miden_objects::block::BlockNumber;
use miden_objects::note::{Note, NoteScript};
use miden_objects::utils::Deserializable;
use miden_objects::utils::sync::LazyLock;
use miden_objects::vm::Program;
use miden_objects::{Felt, Word};

use crate::account::interface::{AccountComponentInterface, AccountInterface};
use crate::account::wallets::BasicWallet;

// WELL KNOWN NOTE SCRIPTS
// ================================================================================================

// Initialize the P2ID note script only once
static P2ID_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/P2ID.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped P2ID script is well-formed");
    NoteScript::new(program)
});

// Initialize the P2IDE note script only once
static P2IDE_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/P2IDE.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped P2IDE script is well-formed");
    NoteScript::new(program)
});

// Initialize the SWAP note script only once
static SWAP_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/SWAP.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped SWAP script is well-formed");
    NoteScript::new(program)
});

/// Returns the P2ID (Pay-to-ID) note script.
fn p2id() -> NoteScript {
    P2ID_SCRIPT.clone()
}

/// Returns the P2ID (Pay-to-ID) note script root.
fn p2id_root() -> Word {
    P2ID_SCRIPT.root()
}

/// Returns the P2IDE (Pay-to-ID with optional reclaim & timelock) note script.
fn p2ide() -> NoteScript {
    P2IDE_SCRIPT.clone()
}

/// Returns the P2IDE (Pay-to-ID with optional reclaim & timelock) note script root.
fn p2ide_root() -> Word {
    P2IDE_SCRIPT.root()
}

/// Returns the SWAP (Swap note) note script.
fn swap() -> NoteScript {
    SWAP_SCRIPT.clone()
}

/// Returns the SWAP (Swap note) note script root.
fn swap_root() -> Word {
    SWAP_SCRIPT.root()
}

// WELL KNOWN NOTE
// ================================================================================================

/// The enum holding the types of basic well-known notes provided by the `miden-lib`.
pub enum WellKnownNote {
    P2ID,
    P2IDE,
    SWAP,
}

impl WellKnownNote {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// Expected number of inputs of the P2ID note.
    const P2ID_NUM_INPUTS: usize = 2;

    /// Expected number of inputs of the P2IDE note.
    const P2IDE_NUM_INPUTS: usize = 4;

    /// Expected number of inputs of the SWAP note.
    const SWAP_NUM_INPUTS: usize = 10;

    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a [WellKnownNote] instance based on the note script of the provided [Note]. Returns
    /// `None` if the provided note is not a basic well-known note.
    pub fn from_note(note: &Note) -> Option<Self> {
        let note_script_root = note.script().root();

        if note_script_root == p2id_root() {
            return Some(Self::P2ID);
        }
        if note_script_root == p2ide_root() {
            return Some(Self::P2IDE);
        }
        if note_script_root == swap_root() {
            return Some(Self::SWAP);
        }

        None
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the expected inputs number of the active note.
    pub fn num_expected_inputs(&self) -> usize {
        match self {
            Self::P2ID => Self::P2ID_NUM_INPUTS,
            Self::P2IDE => Self::P2IDE_NUM_INPUTS,
            Self::SWAP => Self::SWAP_NUM_INPUTS,
        }
    }

    /// Returns the note script of the current [WellKnownNote] instance.
    pub fn script(&self) -> NoteScript {
        match self {
            Self::P2ID => p2id(),
            Self::P2IDE => p2ide(),
            Self::SWAP => swap(),
        }
    }

    /// Returns the script root of the current [WellKnownNote] instance.
    pub fn script_root(&self) -> Word {
        match self {
            Self::P2ID => p2id_root(),
            Self::P2IDE => p2ide_root(),
            Self::SWAP => swap_root(),
        }
    }

    /// Returns a boolean value indicating whether this [WellKnownNote] is compatible with the
    /// provided [AccountInterface].
    pub fn is_compatible_with(&self, account_interface: &AccountInterface) -> bool {
        if account_interface.components().contains(&AccountComponentInterface::BasicWallet) {
            return true;
        }

        let interface_proc_digests = account_interface.get_procedure_digests();
        match self {
            Self::P2ID | &Self::P2IDE => {
                // To consume P2ID and P2IDE notes, the `receive_asset` procedure must be present in
                // the provided account interface.
                interface_proc_digests.contains(&BasicWallet::receive_asset_digest())
            },
            Self::SWAP => {
                // To consume SWAP note, the `receive_asset` and `move_asset_to_note` procedures
                // must be present in the provided account interface.
                interface_proc_digests.contains(&BasicWallet::receive_asset_digest())
                    && interface_proc_digests.contains(&BasicWallet::move_asset_to_note_digest())
            },
        }
    }

    /// Checks the correctness of the provided note inputs against the target account.
    ///
    /// It returns the corresponding note consumption status in case we can guarantee that the note
    /// cannot be consumed, or `None` otherwise.
    ///
    /// It performs:
    /// - for all notes: a
    /// - for `P2ID` note:
    ///     - check that note inputs have correct number of values.
    ///     - assertion that the account ID provided by the note inputs is equal to the target
    ///       account ID.
    /// - for `P2IDE` note:
    ///     - check that note inputs have correct number of values.
    ///     - assertion that the timelock block height is reached, so the note can be consumed.
    ///     - assertion that the account ID provided by the note inputs is equal to the target
    ///       account ID (which means that the note is going to be consumed by the target account)
    ///       or that the target account ID is equal to the sender account ID (which means that the
    ///       note is going to be consumed by the sender account)
    pub fn check_note_inputs(
        &self,
        note: &Note,
        target_account_id: AccountId,
        block_ref: BlockNumber,
    ) -> Option<NoteConsumptionStatus> {
        match self {
            WellKnownNote::P2ID => {
                let note_inputs = note.inputs().values();
                if note_inputs.len() != self.num_expected_inputs() {
                    return Some(NoteConsumptionStatus::Incompatible(format!(
                        "P2ID note should have 2 inputs, but {} was provided",
                        note_inputs.len()
                    )));
                }

                let Some(input_account_id) = try_read_account_id_from_inputs(note_inputs) else {
                    return Some(NoteConsumptionStatus::Incompatible(
                        "Account ID provided to the P2ID note inputs is invalid".to_string(),
                    ));
                };

                if input_account_id == target_account_id {
                    None
                } else {
                    Some(NoteConsumptionStatus::Incompatible("Account ID provided to the P2ID note inputs doesn't match the target account ID".to_string()))
                }
            },
            WellKnownNote::P2IDE => {
                let note_inputs = note.inputs().values();
                if note_inputs.len() != self.num_expected_inputs() {
                    return Some(NoteConsumptionStatus::Incompatible(format!(
                        "P2IDE note should have 4 inputs, but {} was provided",
                        note_inputs.len()
                    )));
                }

                let Some(input_account_id) = try_read_account_id_from_inputs(note_inputs) else {
                    return Some(NoteConsumptionStatus::Incompatible(
                        "Account ID provided to the P2IDE note inputs is invalid".to_string(),
                    ));
                };

                let reclaim_height: Result<u32, _> = note_inputs[2].try_into();
                let Ok(recall_height) = reclaim_height else {
                    return Some(NoteConsumptionStatus::Incompatible("Reclaim block height provided to the P2IDE note inputs should be a u32 value".to_string()));
                };

                let timelock_height: Result<u32, _> = note_inputs[3].try_into();
                let Ok(timelock_height) = timelock_height else {
                    return Some(NoteConsumptionStatus::Incompatible("Timelock block height provided to the P2IDE note inputs should be a u32 value".to_string()));
                };

                if block_ref.as_u32() < timelock_height {
                    return Some(NoteConsumptionStatus::ConsumableAfter(BlockNumber::from(
                        timelock_height,
                    )));
                }

                if block_ref.as_u32() >= recall_height {
                    let sender_account_id = note.metadata().sender();
                    // if the sender can already reclaim the assets back, then:
                    // - target account ID could be equal to the inputs account ID if the note is
                    //   going to be consumed by the target account
                    // - target account ID could be equal to the sender account ID if the note is
                    //   going to be consumed by the sender account
                    if [input_account_id, sender_account_id].contains(&target_account_id) {
                        None
                    } else {
                        Some(NoteConsumptionStatus::Incompatible("Target account of the transaction does not match neither the target account specified by the P2IDE inputs, nor the sender account".to_string()))
                    }
                } else {
                    // in this case note could be consumed only by the target account
                    if input_account_id == target_account_id {
                        None
                    } else {
                        Some(NoteConsumptionStatus::ConsumableAfter(BlockNumber::from(
                            recall_height,
                        )))
                    }
                }
            },
            _ => None,
        }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Reads the account ID from the first two note input values.
///
/// Returns None if the note input values used to construct the account ID are invalid.
fn try_read_account_id_from_inputs(note_inputs: &[Felt]) -> Option<AccountId> {
    let account_id_felts: [Felt; 2] = note_inputs[0..2].try_into().expect(
        "Should be able to convert the first two note inputs to an array of two Felt elements",
    );

    AccountId::try_from([account_id_felts[1], account_id_felts[0]]).ok()
}

// HELPER STRUCTURES
// ================================================================================================

/// Describes if a note could be consumed under a specific conditions: target account state
/// and block height.
///
/// The status does not account for any authorization that may be required to consume the
/// note, nor does it indicate whether the account has sufficient fees to consume it.
#[derive(Debug, PartialEq)]
pub enum NoteConsumptionStatus {
    /// The note can be consumed by the account at the specified block height.
    Consumable,
    /// The note can be consumed by the account after the required block height is achieved.
    ConsumableAfter(BlockNumber),
    /// The note can be consumed by the account if proper authorization is provided.
    ConsumableWithAuthorization,
    /// The note cannot be consumed by the account at the specified conditions (i.e., block
    /// height and account state).
    Unconsumable,
    /// The note cannot be consumed by the specified account under any conditions.
    Incompatible(String),
}

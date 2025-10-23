use alloc::boxed::Box;
use alloc::string::String;
use core::error::Error;

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

    /// Performs the inputs check of the provided note against the target account and the block
    /// number.
    ///
    /// This function returns:
    /// - `Some` if the note can definitely not be consumed under the provided conditions or in
    ///   general.
    /// - `None` if the consumption status of the note cannot be determined conclusively and further
    ///   checks are necessary.
    pub fn check_note_inputs(
        &self,
        note: &Note,
        target_account_id: AccountId,
        block_ref: BlockNumber,
    ) -> Option<NoteConsumptionStatus> {
        match self.check_note_inputs_inner(note, target_account_id, block_ref) {
            Ok(status) => status,
            Err(err) => {
                let err: Box<dyn Error + Send + Sync + 'static> = Box::from(err);
                Some(NoteConsumptionStatus::NeverConsumable(err))
            },
        }
    }

    /// Performs the inputs check of the provided note against the target account and the block
    /// number.
    ///
    /// It performs:
    /// - for `P2ID` note:
    ///     - check that note inputs have correct number of values.
    ///     - assertion that the account ID provided by the note inputs is equal to the target
    ///       account ID.
    /// - for `P2IDE` note:
    ///     - check that note inputs have correct number of values.
    ///     - check that the target account is either the receiver account or the sender account.
    ///     - check that depending on whether the target account is sender or receiver, it could be
    ///       either consumed, or consumed after timelock height, or consumed after reclaim height.
    ///       See the underlying table for the specific cases. Notice that this table defines all
    ///       possible variations of the values of the current block height, timelock height and
    ///       reclaim height relative to each other (it was greatly reduced though).
    /// ```text
    ///       Where:
    ///         - `curr` -- current block height
    ///         - `tl`   -- timelock height
    ///         - `rc`   -- reclaim height
    ///         - `v`    -- symbol to show that heights have any relation
    ///       ┏━━━┳━━━━━━━━━━━━━━━━━━━━┳━━━━━━━━━━━━━┳━━━━━━━━━━━━━┓
    ///       ┃ # ┃  Height sequence   ┃   Sender    ┃  Receiver   ┃
    ///       ┣━━━╋━━━━━━━━━━━━━━━━━━━━╋━━━━━━━━━━━━━╋━━━━━━━━━━━━━┫
    ///       │ 1 │  rc  v  tl  ≤ curr │ return None | return None |
    ///       ├───┼────────────────────┼─────────────┼─────────────┤
    ///       │ 2 │ curr v  rc  <  tl  │  return tl  |  return tl  |
    ///       ├───┼────────────────────┼─────────────┼─────────────┤
    ///       │ 3 │ curr <  tl  ≤  rc  │  return rc  |  return tl  |
    ///       ├───┼────────────────────┼─────────────┼─────────────┤
    ///       | 4 │  tl  ≤ curr <  rc  │  return rc  | return None |
    ///       └───┴────────────────────┴─────────────┴─────────────┘
    /// ```
    fn check_note_inputs_inner(
        &self,
        note: &Note,
        target_account_id: AccountId,
        block_ref: BlockNumber,
    ) -> Result<Option<NoteConsumptionStatus>, StaticAnalysisError> {
        match self {
            WellKnownNote::P2ID => {
                let note_inputs = note.inputs().values();
                if note_inputs.len() != self.num_expected_inputs() {
                    return Err(StaticAnalysisError::new(format!(
                        "P2ID note should have {} inputs, but {} was provided",
                        WellKnownNote::P2ID.num_expected_inputs(),
                        note_inputs.len()
                    )));
                }

                let input_account_id = try_read_account_id_from_inputs(note_inputs)?;

                if input_account_id == target_account_id {
                    Ok(None)
                } else {
                    Ok(Some(NoteConsumptionStatus::NeverConsumable("account ID provided to the P2ID note inputs doesn't match the target account ID".into())))
                }
            },
            WellKnownNote::P2IDE => {
                let note_inputs = note.inputs().values();
                if note_inputs.len() != self.num_expected_inputs() {
                    return Err(StaticAnalysisError::new(format!(
                        "P2IDE note should have {} inputs, but {} was provided",
                        WellKnownNote::P2IDE.num_expected_inputs(),
                        note_inputs.len()
                    )));
                }

                let input_account_id = try_read_account_id_from_inputs(note_inputs)?;

                // if the target account is not the sender of the note or the receiver (from the
                // note's inputs), then this account cannot consume the note
                if ![input_account_id, note.metadata().sender()].contains(&target_account_id) {
                    return Err(StaticAnalysisError::new(
                        "transaction target account doesn't match neither the receiver account specified by the P2IDE inputs, nor the sender account",
                    ));
                }

                let reclaim_height = u32::try_from(note_inputs[2]).map_err(|_err| {
                    StaticAnalysisError::new("reclaim block height should be a u32")
                })?;

                let timelock_height = u32::try_from(note_inputs[3]).map_err(|_err| {
                    StaticAnalysisError::new("timelock block height should be a u32")
                })?;

                let current_block_height = block_ref.as_u32();

                // Handle the case when current block height is greater or equal to the timelock
                // height and reclaim height (first row in the table).
                //
                // Return None: both sender and receiver accounts can consume the note
                if current_block_height >= reclaim_height.max(timelock_height) {
                    return Ok(None);
                }

                // Handle the case when timelock height is greater than the current block height and
                // reclaim height (second row in the table).
                //
                // Return the timelock height: neither sender, nor the target cannot consume the
                // note.
                if timelock_height > current_block_height.max(reclaim_height) {
                    return Ok(Some(NoteConsumptionStatus::ConsumableAfter(BlockNumber::from(
                        timelock_height,
                    ))));
                }

                // Handle the case when the target account is the sender and the reclaim height is
                // strictly greater than the current block height and greater or equal to the
                // timelock height (`Sender` column of the last two rows in the table).
                //
                // Return the reclaim height since for the sender it is only important to know that
                // the reclaim height is not reached yet and the reclaim height is greater than the
                // timelock height (we should wait for the reclaim anyway).
                if target_account_id == note.metadata().sender()
                    && reclaim_height > current_block_height
                    && reclaim_height >= timelock_height
                {
                    Ok(Some(NoteConsumptionStatus::ConsumableAfter(BlockNumber::from(
                        reclaim_height,
                    ))))
                } else {
                    // Receiver doesn't care what is the height of the reclaim block, so it should
                    // be only checked whether the timelock height is greater than the current block
                    // height or not.
                    //
                    // Return timelock if it is greater than the current block height (third row,
                    // `Receiver` column in the table).
                    if timelock_height > current_block_height {
                        Ok(Some(NoteConsumptionStatus::ConsumableAfter(BlockNumber::from(
                            timelock_height,
                        ))))
                    } else {
                        // Return `None` if the current block height is greater or equal to the
                        // timelock height (fourth row, `Receiver` column in the table).
                        Ok(None)
                    }
                }
            },

            // the consumption status of the `SWAP` note cannot be determined by the static
            // analysis, further checks are necessary.
            _ => Ok(None),
        }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Reads the account ID from the first two note input values.
///
/// Returns None if the note input values used to construct the account ID are invalid.
fn try_read_account_id_from_inputs(note_inputs: &[Felt]) -> Result<AccountId, StaticAnalysisError> {
    let account_id_felts: [Felt; 2] = note_inputs[0..2].try_into().map_err(|source| {
        StaticAnalysisError::with_source(
            "should be able to convert the first two note inputs to an array of two Felt elements",
            source,
        )
    })?;

    AccountId::try_from([account_id_felts[1], account_id_felts[0]]).map_err(|source| {
        StaticAnalysisError::with_source(
            "failed to create an account ID from the first two note inputs",
            source,
        )
    })
}

// HELPER STRUCTURES
// ================================================================================================

/// Describes if a note could be consumed under a specific conditions: target account state
/// and block height.
///
/// The status does not account for any authorization that may be required to consume the
/// note, nor does it indicate whether the account has sufficient fees to consume it.
#[derive(Debug)]
pub enum NoteConsumptionStatus {
    /// The note can be consumed by the account at the specified block height.
    Consumable,
    /// The note can be consumed by the account after the required block height is achieved.
    ConsumableAfter(BlockNumber),
    /// The note can be consumed by the account if proper authorization is provided.
    ConsumableWithAuthorization,
    /// The note cannot be consumed by the account at the specified conditions (i.e., block
    /// height and account state).
    UnconsumableConditions,
    /// The note cannot be consumed by the specified account under any conditions.
    NeverConsumable(Box<dyn Error + Send + Sync + 'static>),
}

#[derive(thiserror::Error, Debug)]
#[error("{message}")]
struct StaticAnalysisError {
    /// Stack size of `Box<str>` is smaller than String.
    message: Box<str>,
    /// thiserror will return this when calling Error::source on StaticAnalysisError.
    source: Option<Box<dyn Error + Send + Sync + 'static>>,
}

impl StaticAnalysisError {
    /// Creates a new static analysis error from an error message.
    pub fn new(message: impl Into<String>) -> Self {
        let message: String = message.into();
        Self { message: message.into(), source: None }
    }

    /// Creates a new static analysis error from an error message and a source error.
    pub fn with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        let message: String = message.into();
        Self {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

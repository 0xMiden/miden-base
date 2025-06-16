use miden_objects::{
    Digest, Felt,
    account::AccountId,
    assembly::{ProcedureName, QualifiedProcedureName},
    block::BlockNumber,
    note::{Note, NoteScript},
    utils::{Deserializable, sync::LazyLock},
    vm::Program,
};

use crate::account::{
    components::basic_wallet_library,
    interface::{AccountComponentInterface, AccountInterface, NoteAccountCompatibility},
};

// WELL KNOWN NOTE SCRIPTS
// ================================================================================================

// Initialize the P2ID note script only once
static P2ID_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/P2ID.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped P2ID script is well-formed");
    NoteScript::new(program)
});

// Initialize the P2IDR note script only once
static P2IDR_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/P2IDR.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped P2IDR script is well-formed");
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
fn p2id_root() -> Digest {
    P2ID_SCRIPT.root()
}

/// Returns the P2IDR (Pay-to-ID with recall) note script.
fn p2idr() -> NoteScript {
    P2IDR_SCRIPT.clone()
}

/// Returns the P2IDR (Pay-to-ID with recall) note script root.
fn p2idr_root() -> Digest {
    P2IDR_SCRIPT.root()
}

/// Returns the P2IDE (Pay-to-ID with optional recall & timelock) note script.
fn p2ide() -> NoteScript {
    P2IDE_SCRIPT.clone()
}

/// Returns the P2IDE (Pay-to-ID with optional recall & timelock) note script root.
fn p2ide_root() -> Digest {
    P2IDE_SCRIPT.root()
}

/// Returns the SWAP (Swap note) note script.
fn swap() -> NoteScript {
    SWAP_SCRIPT.clone()
}

/// Returns the SWAP (Swap note) note script root.
fn swap_root() -> Digest {
    SWAP_SCRIPT.root()
}

// WELL KNOWN NOTE
// ================================================================================================

/// The enum holding the types of basic well-known notes provided by the `miden-lib`.
pub enum WellKnownNote {
    P2ID,
    P2IDR,
    P2IDE,
    SWAP,
}

impl WellKnownNote {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// Expected number of inputs of the P2ID note.
    const P2ID_NUM_INPUTS: usize = 2;

    /// Expected number of inputs of the P2IDR note.
    const P2IDR_NUM_INPUTS: usize = 3;

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
        if note_script_root == p2idr_root() {
            return Some(Self::P2IDR);
        }
        if note_script_root == swap_root() {
            return Some(Self::SWAP);
        }

        None
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the expected inputs number of the current note.
    pub fn num_expected_inputs(&self) -> usize {
        match self {
            Self::P2ID => Self::P2ID_NUM_INPUTS,
            Self::P2IDR => Self::P2IDR_NUM_INPUTS,
            Self::P2IDE => Self::P2IDE_NUM_INPUTS,
            Self::SWAP => Self::SWAP_NUM_INPUTS,
        }
    }

    /// Returns the note script of the current [WellKnownNote] instance.
    pub fn script(&self) -> NoteScript {
        match self {
            Self::P2ID => p2id(),
            Self::P2IDR => p2idr(),
            Self::P2IDE => p2ide(),
            Self::SWAP => swap(),
        }
    }

    /// Returns the script root of the current [WellKnownNote] instance.
    pub fn script_root(&self) -> Digest {
        match self {
            Self::P2ID => p2id_root(),
            Self::P2IDR => p2idr_root(),
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
            Self::P2ID | &Self::P2IDR | &Self::P2IDE => {
                // Get the hash of the "receive_asset" procedure and check that this procedure is
                // presented in the provided account interfaces. P2ID and P2IDR notes requires only
                // this procedure to be consumed by the account.
                let receive_asset_proc_name = QualifiedProcedureName::new(
                    Default::default(),
                    ProcedureName::new("receive_asset").unwrap(),
                );
                let node_id = basic_wallet_library().get_export_node_id(&receive_asset_proc_name);
                let receive_asset_digest = basic_wallet_library().mast_forest()[node_id].digest();

                interface_proc_digests.contains(&receive_asset_digest)
            },
            Self::SWAP => {
                // Make sure that all procedures from the basic wallet library are presented in the
                // provided account interfaces. SWAP note requires the whole basic wallet interface
                // to be consumed by the account.
                basic_wallet_library()
                    .mast_forest()
                    .procedure_digests()
                    .all(|proc_digest| interface_proc_digests.contains(&proc_digest))
            },
        }
    }

    /// Checks the correctness of the provided note inputs against the target account.
    ///
    /// It performs:
    /// - for all notes: a check that note inputs have correct number of values.
    /// - for `P2ID` note: assertion that the account ID provided by the note inputs is equal to the
    ///   target account ID.
    /// - for `P2IDR` note: assertion that the account ID provided by the note inputs is equal to
    ///   the target account ID (which means that the note is going to be consumed by the target
    ///   account) or that the target account ID is equal to the sender account ID (which means that
    ///   the note is going to be consumed by the sender account)
    /// - for `P2IDE` note: 
    ///   - assertion that the ID of the account, against which the transaction is being executed,
    ///     is equal to the target account ID specified in the note inputs (which means that the
    ///     note is going to be consumed by the target account) or is equal to the ID of the
    ///     account, which sent this note (which means that the note is going to be consumed by the
    ///     sender account).
    ///   - assertion that the timelock height was reached.
    ///   - assertion that the recall height was reached.
    pub fn check_note_inputs(
        &self,
        note: &Note,
        target_account_id: AccountId,
        block_ref: BlockNumber,
    ) -> NoteAccountCompatibility {
        match self {
            WellKnownNote::P2ID => {
                let note_inputs = note.inputs().values();
                if note_inputs.len() != self.num_expected_inputs() {
                    return NoteAccountCompatibility::No;
                }

                // Return `No` if the note input values used to construct the account ID are invalid
                let Some(input_account_id) = try_read_account_id_from_inputs(note_inputs) else {
                    return NoteAccountCompatibility::No;
                };

                // check that the account ID in the note inputs equal to the target account ID
                if input_account_id == target_account_id {
                    NoteAccountCompatibility::Yes
                } else {
                    NoteAccountCompatibility::No
                }
            },
            WellKnownNote::P2IDR => {
                let note_inputs = note.inputs().values();
                if note_inputs.len() != self.num_expected_inputs() {
                    return NoteAccountCompatibility::No;
                }

                let recall_height: Result<u32, _> = note_inputs[2].try_into();
                // Return `No` if the note input value which represents the recall height is invalid
                let Ok(recall_height) = recall_height else {
                    return NoteAccountCompatibility::No;
                };

                // Return `No` if the note input values used to construct the account ID are invalid
                let Some(input_account_id) = try_read_account_id_from_inputs(note_inputs) else {
                    return NoteAccountCompatibility::No;
                };

                if block_ref.as_u32() >= recall_height {
                    let sender_account_id = note.metadata().sender();
                    // if the sender can already reclaim the assets back, then:
                    // - target account ID could be equal to the inputs account ID if the note is
                    //   going to be consumed by the target account
                    // - target account ID could be equal to the sender account ID if the note is
                    //   going to be consumed by the sender account
                    if [input_account_id, sender_account_id].contains(&target_account_id) {
                        NoteAccountCompatibility::Yes
                    } else {
                        NoteAccountCompatibility::No
                    }
                } else {
                    // in this case note could be consumed only by the target account
                    if input_account_id == target_account_id {
                        NoteAccountCompatibility::Yes
                    } else {
                        NoteAccountCompatibility::No
                    }
                }
            },
            WellKnownNote::P2IDE => {
                let note_inputs = note.inputs().values();
                // check the expected number of inputs
                if note_inputs.len() != self.num_expected_inputs() {
                    return NoteAccountCompatibility::No;
                }

                // parse timelock height
                let timelock_height: Result<u32, _> = note_inputs[3].try_into();
                let Ok(timelock_height) = timelock_height else {
                    return NoteAccountCompatibility::No;
                };

                // timelock check
                if block_ref.as_u32() < timelock_height {
                    // still locked
                    return NoteAccountCompatibility::No;
                }

                let Some(input_account_id) = try_read_account_id_from_inputs(note_inputs) else {
                    return NoteAccountCompatibility::No;
                };
                let sender_account_id = note.metadata().sender();
                let is_target = input_account_id == target_account_id;
                let is_sender = input_account_id == sender_account_id;

                // only target or sender may consume
                if !is_target && !is_sender {
                    return NoteAccountCompatibility::No;
                }

                // reclaim logic
                if is_sender {
                    // parse recall height
                    let recall_height: Result<u32, _> = note_inputs[2].try_into();
                    let Ok(recall_height) = recall_height else {
                        return NoteAccountCompatibility::No;
                    };

                    // sender can reclaim only after recall block height has passed
                    if block_ref.as_u32() >= recall_height {
                        NoteAccountCompatibility::Yes
                    } else {
                        NoteAccountCompatibility::No
                    }
                } else {
                    // target can spend as soon as timelock is over
                    NoteAccountCompatibility::Yes
                }
            },

            WellKnownNote::SWAP => {
                if note.inputs().values().len() != self.num_expected_inputs() {
                    return NoteAccountCompatibility::No;
                }

                NoteAccountCompatibility::Maybe
            },
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

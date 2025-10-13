use miden_objects::Word;
use miden_objects::note::{Note, NoteScript};
use miden_objects::utils::Deserializable;
use miden_objects::utils::sync::LazyLock;
use miden_objects::vm::Program;

use crate::account::faucets::{BasicFungibleFaucet, NetworkFungibleFaucet};
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

// Initialize the MINT note script only once
static MINT_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/MINT.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped MINT script is well-formed");
    NoteScript::new(program)
});

// Initialize the BURN note script only once
static BURN_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/BURN.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped BURN script is well-formed");
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

/// Returns the MINT (Mint note) note script.
fn mint() -> NoteScript {
    MINT_SCRIPT.clone()
}

/// Returns the MINT (Mint note) note script root.
fn mint_root() -> Word {
    MINT_SCRIPT.root()
}

/// Returns the BURN (Burn note) note script.
fn burn() -> NoteScript {
    BURN_SCRIPT.clone()
}

/// Returns the BURN (Burn note) note script root.
fn burn_root() -> Word {
    BURN_SCRIPT.root()
}

// WELL KNOWN NOTE
// ================================================================================================

/// The enum holding the types of basic well-known notes provided by the `miden-lib`.
pub enum WellKnownNote {
    P2ID,
    P2IDE,
    SWAP,
    MINT,
    BURN,
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

    /// Expected number of inputs of the MINT note.
    const MINT_NUM_INPUTS: usize = 9;

    /// Expected number of inputs of the BURN note.
    const BURN_NUM_INPUTS: usize = 0;

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
        if note_script_root == mint_root() {
            return Some(Self::MINT);
        }
        if note_script_root == burn_root() {
            return Some(Self::BURN);
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
            Self::MINT => Self::MINT_NUM_INPUTS,
            Self::BURN => Self::BURN_NUM_INPUTS,
        }
    }

    /// Returns the note script of the current [WellKnownNote] instance.
    pub fn script(&self) -> NoteScript {
        match self {
            Self::P2ID => p2id(),
            Self::P2IDE => p2ide(),
            Self::SWAP => swap(),
            Self::MINT => mint(),
            Self::BURN => burn(),
        }
    }

    /// Returns the script root of the current [WellKnownNote] instance.
    pub fn script_root(&self) -> Word {
        match self {
            Self::P2ID => p2id_root(),
            Self::P2IDE => p2ide_root(),
            Self::SWAP => swap_root(),
            Self::MINT => mint_root(),
            Self::BURN => burn_root(),
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
            Self::MINT => {
                // MINT notes work only with network fungible faucets. The network faucet uses
                // note-based authentication (checking if the note sender equals the faucet owner)
                // to authorize minting, while basic faucets have different mint procedures that
                // are not compatible with MINT notes.
                interface_proc_digests.contains(&NetworkFungibleFaucet::distribute_digest())
            },
            Self::BURN => {
                // BURN notes work with both basic and network fungible faucets because both
                // faucet types export the same burn procedure (i.e., they have the same digest).
                debug_assert_eq!(
                    BasicFungibleFaucet::burn_digest(),
                    NetworkFungibleFaucet::burn_digest(),
                    "Basic and network fungible faucets must have the same burn digest"
                );
                interface_proc_digests.contains(&BasicFungibleFaucet::burn_digest())
                    || interface_proc_digests.contains(&NetworkFungibleFaucet::burn_digest())
            },
        }
    }
}

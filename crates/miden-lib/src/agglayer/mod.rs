use alloc::vec::Vec;

use miden_objects::account::{AccountComponent, StorageSlot};
use miden_objects::assembly::Library;
use miden_objects::note::NoteScript;
use miden_objects::utils::Deserializable;
use miden_objects::utils::sync::LazyLock;
use miden_objects::vm::Program;

pub mod utils;

// AGGLAYER NOTE SCRIPTS
// ================================================================================================

// Initialize the B2AGG note script only once
static B2AGG_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/agglayer/note_scripts/B2AGG.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped B2AGG script is well-formed");
    NoteScript::new(program)
});

/// Returns the B2AGG (Bridge to AggLayer) note script.
pub fn b2agg_script() -> NoteScript {
    B2AGG_SCRIPT.clone()
}

// Initialize the CLAIM note script only once
static CLAIM_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/agglayer/note_scripts/CLAIM.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped CLAIM script is well-formed");
    NoteScript::new(program)
});

/// Returns the CLAIM (Bridge from AggLayer) note script.
pub fn claim_script() -> NoteScript {
    CLAIM_SCRIPT.clone()
}

// Initialize the UPDATE_GER note script only once
static UPDATE_GER_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes =
        include_bytes!(concat!(env!("OUT_DIR"), "/assets/agglayer/note_scripts/UPDATE_GER.masb"));
    let program =
        Program::read_from_bytes(bytes).expect("Shipped UPDATE_GER script is well-formed");
    NoteScript::new(program)
});

/// Returns the UPDATE_GER (Update Global Exit Root) note script.
pub fn update_ger_script() -> NoteScript {
    UPDATE_GER_SCRIPT.clone()
}

// AGGLAYER ACCOUNT COMPONENTS
// ================================================================================================

// Initialize the unified AggLayer library only once
static AGGLAYER_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/agglayer.masl"));
    Library::read_from_bytes(bytes).expect("Shipped AggLayer library is well-formed")
});

/// Returns the unified AggLayer Library containing all agglayer modules.
pub fn agglayer_library() -> Library {
    AGGLAYER_LIBRARY.clone()
}

/// Returns the Bridge Out Library.
///
/// Note: This is now the same as agglayer_library() since all agglayer components
/// are compiled into a single library.
pub fn bridge_out_library() -> Library {
    agglayer_library()
}

/// Returns the Local Exit Tree Library.
///
/// Note: This is now the same as agglayer_library() since all agglayer components
/// are compiled into a single library.
pub fn local_exit_tree_library() -> Library {
    agglayer_library()
}

/// Creates a Local Exit Tree component with the specified storage slots.
///
/// This component uses the local_exit_tree library and can be added to accounts
/// that need to manage local exit tree functionality.
pub fn local_exit_tree_component(storage_slots: Vec<StorageSlot>) -> AccountComponent {
    let library = local_exit_tree_library();

    AccountComponent::new(library, storage_slots)
        .expect("local_exit_tree component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
}

/// Creates a Bridge Out component with the specified storage slots.
///
/// This component uses the bridge_out library and can be added to accounts
/// that need to bridge assets out to the AggLayer.
pub fn bridge_out_component(storage_slots: Vec<StorageSlot>) -> AccountComponent {
    let library = bridge_out_library();

    AccountComponent::new(library, storage_slots)
        .expect("bridge_out component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
}

// Initialize the Bridge In library only once
static BRIDGE_IN_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/agglayer/account_components/bridge_in.masl"
    ));
    Library::read_from_bytes(bytes).expect("Shipped Bridge In library is well-formed")
});

/// Returns the Bridge In Library.
pub fn bridge_in_library() -> Library {
    BRIDGE_IN_LIBRARY.clone()
}

/// Creates a Bridge In component with the specified storage slots.
///
/// This component uses the bridge_in library and can be added to accounts
/// that need to bridge assets in from the AggLayer.
pub fn bridge_in_component(storage_slots: Vec<StorageSlot>) -> AccountComponent {
    let library = bridge_in_library();

    AccountComponent::new(library, storage_slots)
        .expect("bridge_in component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
}

// Initialize the Agglayer Faucet library only once
static AGGLAYER_FAUCET_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/agglayer/account_components/agglayer_faucet.masl"
    ));
    Library::read_from_bytes(bytes).expect("Shipped Agglayer Faucet library is well-formed")
});

/// Returns the Agglayer Faucet Library.
pub fn agglayer_faucet_library() -> Library {
    AGGLAYER_FAUCET_LIBRARY.clone()
}

/// Creates an Agglayer Faucet component with the specified storage slots.
///
/// This component combines network faucet functionality with bridge validation
/// via Foreign Procedure Invocation (FPI). It provides a "claim" procedure that
/// validates CLAIM notes against a bridge MMR account before minting assets.
pub fn agglayer_faucet_component(storage_slots: Vec<StorageSlot>) -> AccountComponent {
    let library = agglayer_faucet_library();

    AccountComponent::new(library, storage_slots)
        .expect("agglayer_faucet component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
}

/// Creates a combined Bridge Out component that includes both bridge_out and local_exit_tree
/// modules.
///
/// This is a convenience function that creates a component with multiple modules.
/// For more fine-grained control, use the individual component functions and combine them
/// using the AccountBuilder pattern.
pub fn bridge_out_with_local_exit_tree_component(
    storage_slots: Vec<StorageSlot>,
) -> Vec<AccountComponent> {
    vec![
        bridge_out_component(storage_slots.clone()),
        local_exit_tree_component(vec![]), // local_exit_tree typically doesn't need storage slots
    ]
}

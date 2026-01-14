#![no_std]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use miden_assembly::Library;
use miden_assembly::utils::Deserializable;
use miden_core::{Felt, FieldElement, Program, Word};
use miden_protocol::account::{
    Account,
    AccountBuilder,
    AccountComponent,
    AccountId,
    AccountStorageMode,
    AccountType,
    StorageSlot,
    StorageSlotName,
};
use miden_protocol::asset::TokenSymbol;
use miden_protocol::note::NoteScript;
use miden_standards::account::auth::NoAuth;
use miden_standards::account::faucets::NetworkFungibleFaucet;
use miden_utils_sync::LazyLock;

pub mod claim_note;
pub mod errors;
pub mod eth_address_format;
pub mod utils;

// Re-export the main types from claim_note module
pub use claim_note::{
    ClaimNoteParams,
    LeafData,
    OutputNoteData,
    ProofData,
    create_claim_note,
    hex_string_to_address,
    hex_string_to_bytes32,
    hex_strings_to_bytes32_array,
    metadata_hex_to_u32_array,
    string_to_u256_array,
};
pub use eth_address_format::EthAddressFormat;

// AGGLAYER NOTE SCRIPTS
// ================================================================================================

// Initialize the B2AGG note script only once
static B2AGG_SCRIPT: LazyLock<Program> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/B2AGG.masb"));
    Program::read_from_bytes(bytes).expect("Shipped B2AGG script is well-formed")
});

/// Returns the B2AGG (Bridge to AggLayer) note script.
pub fn b2agg_script() -> Program {
    B2AGG_SCRIPT.clone()
}

// Initialize the CLAIM note script only once
static CLAIM_SCRIPT: LazyLock<NoteScript> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/assets/note_scripts/CLAIM.masb"));
    let program = Program::read_from_bytes(bytes).expect("Shipped CLAIM script is well-formed");
    NoteScript::new(program)
});

/// Returns the CLAIM (Bridge from AggLayer) note script.
pub fn claim_script() -> NoteScript {
    CLAIM_SCRIPT.clone()
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

/// Returns the Bridge In Library.
///
/// Note: This is now the same as agglayer_library() since all agglayer components
/// are compiled into a single library.
pub fn bridge_in_library() -> Library {
    agglayer_library()
}

/// Creates a Bridge In component with the specified storage slots.
///
/// This component uses the agglayer library and can be added to accounts
/// that need to bridge assets in from the AggLayer.
pub fn bridge_in_component(storage_slots: Vec<StorageSlot>) -> AccountComponent {
    let library = bridge_in_library();

    AccountComponent::new(library, storage_slots)
        .expect("bridge_in component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
}

/// Returns the Agglayer Faucet Library.
///
/// Note: This is now the same as agglayer_library() since all agglayer components
/// are compiled into a single library.
pub fn agglayer_faucet_library() -> Library {
    agglayer_library()
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

/// Creates an Asset Conversion component with the specified storage slots.
///
/// This component uses the agglayer library (which includes asset_conversion) and can be added to
/// accounts that need to convert assets between Miden and Ethereum formats.
pub fn asset_conversion_component(storage_slots: Vec<StorageSlot>) -> AccountComponent {
    let library = agglayer_library();

    AccountComponent::new(library, storage_slots)
        .expect("asset_conversion component should satisfy the requirements of a valid account component")
        .with_supports_all_types()
}

// AGGLAYER ACCOUNT CREATION HELPERS
// ================================================================================================

/// Creates a bridge account component with the standard bridge storage slot.
///
/// This is a convenience function that creates the bridge storage slot with the standard
/// name "miden::agglayer::bridge" and returns the bridge_out component.
///
/// # Returns
/// Returns an [`AccountComponent`] configured for bridge operations with MMR validation.
pub fn create_bridge_account_component() -> AccountComponent {
    let bridge_storage_slot_name = StorageSlotName::new("miden::agglayer::bridge")
        .expect("Bridge storage slot name should be valid");
    let bridge_storage_slots = vec![StorageSlot::with_empty_map(bridge_storage_slot_name)];
    bridge_out_component(bridge_storage_slots)
}

/// Creates an agglayer faucet account component with the specified configuration.
///
/// This function creates all the necessary storage slots for an agglayer faucet:
/// - Network faucet metadata slot (max_supply, decimals, token_symbol)
/// - Bridge account reference slot for FPI validation
///
/// # Parameters
/// - `token_symbol`: The symbol for the fungible token (e.g., "AGG")
/// - `decimals`: Number of decimal places for the token
/// - `max_supply`: Maximum supply of the token
/// - `bridge_account_id`: The account ID of the bridge account for validation
///
/// # Returns
/// Returns an [`AccountComponent`] configured for agglayer faucet operations.
///
/// # Panics
/// Panics if the token symbol is invalid or storage slot names are malformed.
pub fn create_agglayer_faucet_component(
    token_symbol: &str,
    decimals: u8,
    max_supply: Felt,
    bridge_account_id: AccountId,
) -> AccountComponent {
    // Create network faucet metadata slot: [max_supply, decimals, token_symbol, 0]
    let token_symbol = TokenSymbol::new(token_symbol).expect("Token symbol should be valid");
    let metadata_word =
        Word::new([max_supply, Felt::from(decimals), token_symbol.into(), FieldElement::ZERO]);
    let metadata_slot =
        StorageSlot::with_value(NetworkFungibleFaucet::metadata_slot().clone(), metadata_word);

    // Create agglayer-specific bridge storage slot
    let bridge_account_id_word = Word::new([
        Felt::new(0),
        Felt::new(0),
        bridge_account_id.suffix(),
        bridge_account_id.prefix().as_felt(),
    ]);
    let agglayer_storage_slot_name = StorageSlotName::new("miden::agglayer::faucet")
        .expect("Agglayer faucet storage slot name should be valid");
    let bridge_slot = StorageSlot::with_value(agglayer_storage_slot_name, bridge_account_id_word);

    // Combine all storage slots for the agglayer faucet component
    let agglayer_storage_slots = vec![metadata_slot, bridge_slot];
    agglayer_faucet_component(agglayer_storage_slots)
}

/// Creates a complete bridge account builder with the standard configuration.
pub fn create_bridge_account_builder(seed: Word) -> AccountBuilder {
    let bridge_component = create_bridge_account_component();
    Account::builder(seed.into())
        .storage_mode(AccountStorageMode::Public)
        .with_component(bridge_component)
}

/// Creates a new bridge account with the standard configuration.
///
/// This creates a new account suitable for production use.
pub fn create_bridge_account(seed: Word) -> Account {
    create_bridge_account_builder(seed)
        .with_auth_component(AccountComponent::from(NoAuth))
        .build()
        .expect("Bridge account should be valid")
}

/// Creates an existing bridge account with the standard configuration.
///
/// This creates an existing account suitable for testing scenarios.
#[cfg(any(feature = "testing", test))]
pub fn create_existing_bridge_account(seed: Word) -> Account {
    create_bridge_account_builder(seed)
        .with_auth_component(AccountComponent::from(NoAuth))
        .build_existing()
        .expect("Bridge account should be valid")
}

/// Creates a complete agglayer faucet account builder with the specified configuration.
pub fn create_agglayer_faucet_builder(
    seed: Word,
    token_symbol: &str,
    decimals: u8,
    max_supply: Felt,
    bridge_account_id: AccountId,
) -> AccountBuilder {
    let agglayer_component =
        create_agglayer_faucet_component(token_symbol, decimals, max_supply, bridge_account_id);

    Account::builder(seed.into())
        .account_type(AccountType::FungibleFaucet)
        .storage_mode(AccountStorageMode::Network)
        .with_component(agglayer_component)
}

/// Creates a new agglayer faucet account with the specified configuration.
///
/// This creates a new account suitable for production use.
pub fn create_agglayer_faucet(
    seed: Word,
    token_symbol: &str,
    decimals: u8,
    max_supply: Felt,
    bridge_account_id: AccountId,
) -> Account {
    create_agglayer_faucet_builder(seed, token_symbol, decimals, max_supply, bridge_account_id)
        .with_auth_component(AccountComponent::from(NoAuth))
        .build()
        .expect("Agglayer faucet account should be valid")
}

/// Creates an existing agglayer faucet account with the specified configuration.
///
/// This creates an existing account suitable for testing scenarios.
#[cfg(any(feature = "testing", test))]
pub fn create_existing_agglayer_faucet(
    seed: Word,
    token_symbol: &str,
    decimals: u8,
    max_supply: Felt,
    bridge_account_id: AccountId,
) -> Account {
    create_agglayer_faucet_builder(seed, token_symbol, decimals, max_supply, bridge_account_id)
        .with_auth_component(AccountComponent::from(NoAuth))
        .build_existing()
        .expect("Agglayer faucet account should be valid")
}

// TESTING HELPERS
// ================================================================================================

#[cfg(any(feature = "testing", test))]
/// Type alias for the complex return type of claim_note_test_inputs.
///
/// Contains native types for the new ClaimNoteParams structure:
/// - smt_proof_local_exit_root: `Vec<[u8; 32]>` (256 bytes32 values)
/// - smt_proof_rollup_exit_root: `Vec<[u8; 32]>` (256 bytes32 values)
/// - global_index: [u32; 8]
/// - mainnet_exit_root: [u8; 32]
/// - rollup_exit_root: [u8; 32]
/// - origin_network: u32
/// - origin_token_address: [u8; 20]
/// - destination_network: u32
/// - destination_address: [u8; 20]
/// - amount: [u32; 8]
/// - metadata: [u32; 8]
pub type ClaimNoteTestInputs = (
    Vec<[u8; 32]>,
    Vec<[u8; 32]>,
    [u32; 8],
    [u8; 32],
    [u8; 32],
    u32,
    [u8; 20],
    u32,
    [u8; 20],
    [u32; 8],
    [u32; 8],
);

#[cfg(any(feature = "testing", test))]
/// Returns dummy test inputs for creating CLAIM notes with native types.
///
/// This is a convenience function for testing that provides realistic dummy data
/// for all the agglayer claimAsset function inputs using native types.
///
/// # Parameters
/// - `amount`: The amount as a single u32 for the test
/// - `destination_account_id`: The destination account ID to convert to address bytes
///
/// # Returns
/// A tuple containing native types for the new ClaimNoteParams structure
pub fn claim_note_test_inputs(
    amount: u32,
    destination_account_id: AccountId,
) -> ClaimNoteTestInputs {
    // Create SMT proofs with 256 bytes32 values each
    let smt_proof_local_exit_root = vec![[0u8; 32]; 256];
    let smt_proof_rollup_exit_root = vec![[0u8; 32]; 256];
    let global_index = [12345u32, 0, 0, 0, 0, 0, 0, 0];

    let mainnet_exit_root: [u8; 32] = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
        0x77, 0x88,
    ];

    let rollup_exit_root: [u8; 32] = [
        0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
        0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99,
    ];

    let origin_network = 1u32;

    let origin_token_address: [u8; 20] = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99, 0xaa, 0xbb, 0xcc,
    ];

    let destination_network = 2u32;

    // Convert AccountId to destination address bytes using EthAddressFormat
    let destination_address =
        EthAddressFormat::from_account_id(destination_account_id).into_bytes();

    // Convert amount to u32 array for agglayer
    let amount_u32 = [amount, 0, 0, 0, 0, 0, 0, 0];
    let metadata: [u32; 8] = [0; 8];

    (
        smt_proof_local_exit_root,
        smt_proof_rollup_exit_root,
        global_index,
        mainnet_exit_root,
        rollup_exit_root,
        origin_network,
        origin_token_address,
        destination_network,
        destination_address,
        amount_u32,
        metadata,
    )
}

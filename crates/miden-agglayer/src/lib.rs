#![no_std]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use miden_assembly::Library;
use miden_assembly::utils::Deserializable;
use miden_core::{Felt, FieldElement, Program, Word};
use miden_protocol::NoteError;
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
use miden_protocol::crypto::rand::FeltRng;
use miden_protocol::note::{
    Note,
    NoteAssets,
    NoteExecutionHint,
    NoteInputs,
    NoteMetadata,
    NoteRecipient,
    NoteScript,
    NoteTag,
    NoteType,
};
use miden_standards::account::auth::NoAuth;
use miden_standards::account::faucets::NetworkFungibleFaucet;
use miden_utils_sync::LazyLock;

pub mod errors;
pub mod utils;

use utils::{bytes32_to_felts, ethereum_address_to_felts};

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

// AGGLAYER NOTE CREATION HELPERS
// ================================================================================================

/// Parameters for creating a CLAIM note.
///
/// This struct groups all the parameters needed to create a CLAIM note that instructs
/// an agglayer faucet to validate and mint assets. The parameters match the Solidity
/// bridge contract's claimAsset function signature.
pub struct ClaimNoteParams<'a, R: FeltRng> {
    /// The account ID of the agglayer faucet that will process the claim
    pub agg_faucet_id: AccountId,
    /// The account ID of the note creator
    pub sender: AccountId,
    /// The account ID that will receive the P2ID output note
    pub target_account_id: AccountId,
    /// The amount of assets to be minted and transferred
    pub amount: Felt,
    /// The serial number for the output note
    pub output_serial_num: Word,
    /// Auxiliary data for the CLAIM note (verified against Global Exit Tree)
    pub aux: Felt,
    /// SMT proof data (570 felts) matching Solidity claimAsset smtProof parameter
    pub smt_proof: Vec<Felt>,
    /// Index for the claim (u32 as Felt)
    pub index: Felt,
    /// Mainnet exit root hash (bytes32 as 32-byte array)
    pub mainnet_exit_root: &'a [u8; 32],
    /// Rollup exit root hash (bytes32 as 32-byte array)
    pub rollup_exit_root: &'a [u8; 32],
    /// Origin network identifier (u32 as Felt)
    pub origin_network: Felt,
    /// Origin token address (address as 20-byte array)
    pub origin_token_address: &'a [u8; 20],
    /// Destination network identifier (u32 as Felt)
    pub destination_network: Felt,
    /// Destination address (address as 20-byte array)
    pub destination_address: &'a [u8; 20],
    /// Additional metadata for the claim (fixed size of 8 felts)
    pub metadata: [Felt; 8],
    /// Random number generator for creating the serial number
    pub rng: &'a mut R,
}

/// Generates a CLAIM note - a note that instructs an agglayer faucet to validate and mint assets.
///
/// # Parameters
/// - `params`: The parameters for creating the CLAIM note (including RNG)
///
/// # Errors
/// Returns an error if note creation fails.
pub fn create_claim_note<R: FeltRng>(params: ClaimNoteParams<'_, R>) -> Result<Note, NoteError> {
    // Validate SMT proof length
    if params.smt_proof.len() != 570 {
        return Err(NoteError::other(alloc::format!(
            "SMT proof must be exactly 570 felts, got {}",
            params.smt_proof.len()
        )));
    }

    let claim_script = claim_script();
    let serial_num = params.rng.draw_word();

    let note_type = NoteType::Public;
    let execution_hint = NoteExecutionHint::always();

    let output_note_tag = NoteTag::from_account_id(params.target_account_id);

    let mut claim_inputs = vec![
        Felt::new(0),                                // execution_hint (always = 0)
        params.aux,                                  // aux
        Felt::from(output_note_tag),                 // tag
        Felt::ZERO,                                  // padding to be word-aligned
        params.output_serial_num[0],                 // SERIAL_NUM[0]
        params.output_serial_num[1],                 // SERIAL_NUM[1]
        params.output_serial_num[2],                 // SERIAL_NUM[2]
        params.output_serial_num[3],                 // SERIAL_NUM[3]
        params.target_account_id.suffix(),           // P2ID input: suffix
        params.target_account_id.prefix().as_felt(), // P2ID input: prefix
        params.agg_faucet_id.suffix(),               // faucet account suffix
        params.agg_faucet_id.prefix().as_felt(),     // faucet account prefix
    ];

    // Add Solidity claimAsset function parameters in order:
    // smtProof (570 felts) - comes first to match Solidity parameter order
    claim_inputs.extend(params.smt_proof);

    // index (u32 as Felt)
    claim_inputs.push(params.index);

    // mainnetExitRoot (bytes32 as 8 u32 felts)
    let mainnet_exit_root_felts = bytes32_to_felts(params.mainnet_exit_root);
    claim_inputs.extend(mainnet_exit_root_felts);

    // rollupExitRoot (bytes32 as 8 u32 felts)
    let rollup_exit_root_felts = bytes32_to_felts(params.rollup_exit_root);
    claim_inputs.extend(rollup_exit_root_felts);

    // originNetwork (u32 as Felt)
    claim_inputs.push(params.origin_network);

    // originTokenAddress (address as 5 u32 felts)
    let origin_token_address_felts = ethereum_address_to_felts(params.origin_token_address);
    claim_inputs.extend(origin_token_address_felts);

    // destinationNetwork (u32 as Felt)
    claim_inputs.push(params.destination_network);

    // destinationAddress (address as 5 u32 felts)
    let destination_address_felts = ethereum_address_to_felts(params.destination_address);
    claim_inputs.extend(destination_address_felts);

    // amount to claim
    claim_inputs.push(params.amount);

    // metadata
    claim_inputs.extend(params.metadata);

    let inputs = NoteInputs::new(claim_inputs)?;
    let tag = NoteTag::from_account_id(params.agg_faucet_id);
    let metadata = NoteMetadata::new(params.sender, note_type, tag, execution_hint, params.aux)?;
    let assets = NoteAssets::new(vec![])?;
    let recipient = NoteRecipient::new(serial_num, claim_script, inputs);

    Ok(Note::new(assets, metadata, recipient))
}

// TESTING HELPERS
// ================================================================================================

#[cfg(any(feature = "testing", test))]
/// Type alias for the complex return type of claim_note_test_inputs.
///
/// Contains:
/// - smt_proof: Vec<Felt> (570 felts)
/// - index: Felt
/// - mainnet_exit_root: [u8; 32]
/// - rollup_exit_root: [u8; 32]
/// - origin_network: Felt
/// - origin_token_address: [u8; 20]
/// - destination_network: Felt
/// - destination_address: [u8; 20]
/// - metadata: [Felt; 8]
pub type ClaimNoteTestInputs =
    (Vec<Felt>, Felt, [u8; 32], [u8; 32], Felt, [u8; 20], Felt, [u8; 20], [Felt; 8]);

#[cfg(any(feature = "testing", test))]
/// Returns dummy test inputs for creating CLAIM notes.
///
/// This is a convenience function for testing that provides realistic dummy data
/// for all the Solidity bridge inputs.
///
/// # Returns
/// A tuple containing:
/// - smt_proof: Vec<Felt> (570 felts)
/// - index: Felt
/// - mainnet_exit_root: [u8; 32]
/// - rollup_exit_root: [u8; 32]
/// - origin_network: Felt
/// - origin_token_address: [u8; 20]
/// - destination_network: Felt
/// - destination_address: [u8; 20]
/// - metadata: [Felt; 8]
pub fn claim_note_test_inputs() -> ClaimNoteTestInputs {
    // Create SMT proof with 570 felts (matching Solidity smtProof parameter)
    let smt_proof = vec![Felt::new(0); 570];
    let index = Felt::new(12345);

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

    let origin_network = Felt::new(1);

    let origin_token_address: [u8; 20] = [
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77,
        0x88, 0x99, 0xaa, 0xbb, 0xcc,
    ];

    let destination_network = Felt::new(2);

    let destination_address: [u8; 20] = [
        0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
        0x99, 0xaa, 0xbb, 0xcc, 0xdd,
    ];

    let metadata: [Felt; 8] = [Felt::new(0); 8];
    (
        smt_proof,
        index,
        mainnet_exit_root,
        rollup_exit_root,
        origin_network,
        origin_token_address,
        destination_network,
        destination_address,
        metadata,
    )
}

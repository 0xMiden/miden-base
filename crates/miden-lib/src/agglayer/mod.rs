use alloc::vec::Vec;

use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountComponent,
    AccountId,
    AccountStorageMode,
    AccountType,
    StorageSlot,
    StorageSlotName,
};
use miden_objects::assembly::Library;
use miden_objects::asset::TokenSymbol;
use miden_objects::crypto::rand::FeltRng;
use miden_objects::note::{
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
use miden_objects::utils::Deserializable;
use miden_objects::utils::sync::LazyLock;
use miden_objects::vm::Program;
use miden_objects::{Felt, FieldElement, NoteError, Word};

use crate::account::faucets::NetworkFungibleFaucet;

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

// Initialize the Asset Conversion library only once
static ASSET_CONVERSION_LIBRARY: LazyLock<Library> = LazyLock::new(|| {
    let bytes = include_bytes!(concat!(
        env!("OUT_DIR"),
        "/assets/agglayer/account_components/asset_conversion.masl"
    ));
    Library::read_from_bytes(bytes).expect("Shipped Asset Conversion library is well-formed")
});

/// Returns the Asset Conversion Library.
pub fn asset_conversion_library() -> Library {
    ASSET_CONVERSION_LIBRARY.clone()
}

/// Creates an Asset Conversion component with the specified storage slots.
///
/// This component uses the asset_conversion library and can be added to accounts
/// that need to convert assets between Miden and Ethereum formats.
pub fn asset_conversion_component(storage_slots: Vec<StorageSlot>) -> AccountComponent {
    let library = asset_conversion_library();

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
///
/// This is a high-level convenience function that creates a bridge account with:
/// - Public storage mode for MMR validation
/// - Bridge out component with standard bridge storage slot
/// - Random account seed
///
/// # Parameters
/// - `seed`: The account seed for deterministic account creation
///
/// # Returns
/// Returns an [`miden_objects::account::AccountBuilder`] ready to be built into a bridge account.
///
/// # Example
/// ```rust,ignore
/// use miden_lib::agglayer::create_bridge_account_builder;
/// use miden_objects::crypto::rand::RpoRandomCoin;
///
/// let mut rng = RpoRandomCoin::new([0u8; 32]);
/// let seed = rng.draw_word();
/// let bridge_builder = create_bridge_account_builder(seed);
/// ```
pub fn create_bridge_account_builder(seed: Word) -> miden_objects::account::AccountBuilder {
    let bridge_component = create_bridge_account_component();
    Account::builder(seed.into())
        .storage_mode(AccountStorageMode::Public)
        .with_component(bridge_component)
}

/// Creates a complete agglayer faucet account builder with the specified configuration.
///
/// This is a high-level convenience function that creates an agglayer faucet account with:
/// - FungibleFaucet account type
/// - Network storage mode (required for network faucets)
/// - Agglayer faucet component with all necessary storage slots
///
/// # Parameters
/// - `seed`: The account seed for deterministic account creation
/// - `token_symbol`: The symbol for the fungible token (e.g., "AGG")
/// - `decimals`: Number of decimal places for the token
/// - `max_supply`: Maximum supply of the token
/// - `bridge_account_id`: The account ID of the bridge account for validation
///
/// # Returns
/// Returns an [`miden_objects::account::AccountBuilder`] ready to be built into an agglayer faucet
/// account.
///
/// # Example
/// ```rust,ignore
/// use miden_lib::agglayer::create_agglayer_faucet_builder;
/// use miden_objects::crypto::rand::RpoRandomCoin;
/// use miden_objects::{Felt, AccountId};
///
/// let mut rng = RpoRandomCoin::new([0u8; 32]);
/// let seed = rng.draw_word();
/// let bridge_id = AccountId::try_from(0x1234567890abcdef_u64).unwrap();
/// let max_supply = Felt::new(1000000);
///
/// let faucet_builder = create_agglayer_faucet_builder(
///     seed,
///     "AGG",
///     8,
///     max_supply,
///     bridge_id,
/// );
/// ```
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

// AGGLAYER NOTE CREATION HELPERS
// ================================================================================================

/// Generates a CLAIM note - a note that instructs an agglayer faucet to validate and mint assets.
///
/// # Parameters
/// - `faucet_id`: The account ID of the agglayer faucet that will process the claim
/// - `sender`: The account ID of the note creator
/// - `target_account_id`: The account ID that will receive the P2ID output note
/// - `amount`: The amount of assets to be minted and transferred
/// - `output_note_script`: The script for the output note (typically P2ID script)
/// - `output_serial_num`: The serial number for the output note
/// - `aux`: Auxiliary data for the CLAIM note (verified against Global Exit Tree)
/// - `rng`: Random number generator for creating the serial number
///
/// # Errors
/// Returns an error if note creation fails.
pub fn create_claim_note<R: FeltRng>(
    agg_faucet_id: AccountId,
    sender: AccountId,
    target_account_id: AccountId,
    amount: Felt,
    output_note_script: &NoteScript,
    output_serial_num: Word,
    aux: Felt,
    rng: &mut R,
) -> Result<Note, NoteError> {
    let claim_script = claim_script();
    let serial_num = rng.draw_word();

    let note_type = NoteType::Public;
    let execution_hint = NoteExecutionHint::always();

    let output_note_tag = NoteTag::from_account_id(target_account_id);

    let claim_inputs = vec![
        Felt::new(0),                         // execution_hint (always = 0)
        aux,                                  // aux
        Felt::from(output_note_tag),          // tag
        amount,                               // amount
        output_note_script.root()[0],         // SCRIPT_ROOT[0]
        output_note_script.root()[1],         // SCRIPT_ROOT[1]
        output_note_script.root()[2],         // SCRIPT_ROOT[2]
        output_note_script.root()[3],         // SCRIPT_ROOT[3]
        output_serial_num[0],                 // SERIAL_NUM[0]
        output_serial_num[1],                 // SERIAL_NUM[1]
        output_serial_num[2],                 // SERIAL_NUM[2]
        output_serial_num[3],                 // SERIAL_NUM[3]
        target_account_id.suffix(),           // P2ID input: suffix
        target_account_id.prefix().as_felt(), // P2ID input: prefix
    ];

    let inputs = NoteInputs::new(claim_inputs)?;
    let tag = NoteTag::from_account_id(agg_faucet_id);
    let metadata = NoteMetadata::new(sender, note_type, tag, execution_hint, aux)?;
    let assets = NoteAssets::new(vec![])?;
    let recipient = NoteRecipient::new(serial_num, claim_script, inputs);

    Ok(Note::new(assets, metadata, recipient))
}

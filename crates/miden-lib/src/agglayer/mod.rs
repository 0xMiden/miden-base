use alloc::vec::Vec;

use miden_objects::account::{AccountComponent, StorageSlot};
use miden_objects::assembly::Library;
use miden_objects::utils::Deserializable;
use miden_objects::utils::sync::LazyLock;

pub mod utils;

// AGGLAYER ACCOUNT COMPONENTS
// ================================================================================================

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

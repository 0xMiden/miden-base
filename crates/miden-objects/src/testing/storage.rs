use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use miden_core::{Felt, Word};
use miden_crypto::EMPTY_WORD;

use crate::AccountDeltaError;
use crate::account::{
    AccountStorage,
    AccountStorageDelta,
    NamedStorageSlot,
    SlotName,
    StorageMap,
    StorageMapDelta,
};
use crate::note::NoteAssets;
use crate::utils::sync::LazyLock;

// ACCOUNT STORAGE DELTA BUILDER
// ================================================================================================

#[derive(Clone, Debug, Default)]
pub struct AccountStorageDeltaBuilder {
    values: BTreeMap<SlotName, Word>,
    maps: BTreeMap<SlotName, StorageMapDelta>,
}

impl AccountStorageDeltaBuilder {
    // CONSTRUCTORS
    // -------------------------------------------------------------------------------------------

    pub fn new() -> Self {
        Self {
            values: BTreeMap::new(),
            maps: BTreeMap::new(),
        }
    }

    // MODIFIERS
    // -------------------------------------------------------------------------------------------

    pub fn add_cleared_items(mut self, items: impl IntoIterator<Item = SlotName>) -> Self {
        self.values.extend(items.into_iter().map(|slot| (slot, EMPTY_WORD)));
        self
    }

    pub fn add_updated_values(mut self, items: impl IntoIterator<Item = (SlotName, Word)>) -> Self {
        self.values.extend(items);
        self
    }

    pub fn add_updated_maps(
        mut self,
        items: impl IntoIterator<Item = (SlotName, StorageMapDelta)>,
    ) -> Self {
        self.maps.extend(items);
        self
    }

    // BUILDERS
    // -------------------------------------------------------------------------------------------

    pub fn build(self) -> Result<AccountStorageDelta, AccountDeltaError> {
        AccountStorageDelta::from_parts(self.values, self.maps)
    }
}

// CONSTANTS
// ================================================================================================

pub static MOCK_VALUE_SLOT0: LazyLock<SlotName> =
    LazyLock::new(|| SlotName::new("miden::test::value0").expect("slot name should be valid"));
pub static MOCK_VALUE_SLOT1: LazyLock<SlotName> =
    LazyLock::new(|| SlotName::new("miden::test::value1").expect("slot name should be valid"));
pub static MOCK_MAP_SLOT: LazyLock<SlotName> =
    LazyLock::new(|| SlotName::new("miden::test::map").expect("slot name should be valid"));

pub const STORAGE_VALUE_0: Word =
    Word::new([Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]);
pub const STORAGE_VALUE_1: Word =
    Word::new([Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)]);
pub const STORAGE_LEAVES_2: [(Word, Word); 2] = [
    (
        Word::new([Felt::new(101), Felt::new(102), Felt::new(103), Felt::new(104)]),
        Word::new([Felt::new(1_u64), Felt::new(2_u64), Felt::new(3_u64), Felt::new(4_u64)]),
    ),
    (
        Word::new([Felt::new(105), Felt::new(106), Felt::new(107), Felt::new(108)]),
        Word::new([Felt::new(5_u64), Felt::new(6_u64), Felt::new(7_u64), Felt::new(8_u64)]),
    ),
];

impl AccountStorage {
    /// Create account storage.
    pub fn mock() -> Self {
        AccountStorage::new(Self::mock_storage_slots()).unwrap()
    }

    pub fn mock_storage_slots() -> Vec<NamedStorageSlot> {
        vec![Self::mock_value_slot0(), Self::mock_value_slot1(), Self::mock_map_slot()]
    }

    pub fn mock_value_slot0() -> NamedStorageSlot {
        NamedStorageSlot::with_value(MOCK_VALUE_SLOT0.clone(), STORAGE_VALUE_0)
    }

    pub fn mock_value_slot1() -> NamedStorageSlot {
        NamedStorageSlot::with_value(MOCK_VALUE_SLOT1.clone(), STORAGE_VALUE_1)
    }

    pub fn mock_map_slot() -> NamedStorageSlot {
        NamedStorageSlot::with_map(MOCK_MAP_SLOT.clone(), Self::mock_map())
    }

    pub fn mock_map() -> StorageMap {
        StorageMap::with_entries(STORAGE_LEAVES_2).unwrap()
    }
}

// UTILITIES
// --------------------------------------------------------------------------------------------

/// Returns a list of strings, one for each note asset.
pub fn prepare_assets(note_assets: &NoteAssets) -> Vec<String> {
    let mut assets = Vec::new();
    for &asset in note_assets.iter() {
        let asset_word = Word::from(asset);
        assets.push(asset_word.to_string());
    }
    assets
}

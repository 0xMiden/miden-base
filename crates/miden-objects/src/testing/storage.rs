use alloc::{collections::BTreeMap, string::String, vec::Vec};

use miden_crypto::EMPTY_WORD;
use vm_core::{Felt, Word};

use crate::{
    AccountDeltaError,
    account::{AccountStorage, AccountStorageDelta, StorageMap, StorageMapDelta, StorageSlot},
    note::NoteAssets,
    utils::word_to_masm_push_string,
};

// ACCOUNT STORAGE DELTA BUILDER
// ================================================================================================

#[derive(Clone, Debug, Default)]
pub struct AccountStorageDeltaBuilder {
    values: BTreeMap<u8, Word>,
    maps: BTreeMap<u8, StorageMapDelta>,
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

    pub fn add_cleared_items(mut self, items: impl IntoIterator<Item = u8>) -> Self {
        self.values.extend(items.into_iter().map(|slot| (slot, EMPTY_WORD)));
        self
    }

    pub fn add_updated_values(mut self, items: impl IntoIterator<Item = (u8, Word)>) -> Self {
        self.values.extend(items);
        self
    }

    pub fn add_updated_maps(
        mut self,
        items: impl IntoIterator<Item = (u8, StorageMapDelta)>,
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

// ACCOUNT STORAGE UTILS
// ================================================================================================

pub struct SlotWithIndex {
    pub slot: StorageSlot,
    pub index: u8,
}

// CONSTANTS
// ================================================================================================

pub const FAUCET_STORAGE_DATA_SLOT: u8 = 0;

pub const STORAGE_INDEX_0: u8 = 0;
pub const STORAGE_INDEX_1: u8 = 1;
pub const STORAGE_INDEX_2: u8 = 2;

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
    /// Create account storage:
    pub fn mock() -> Self {
        AccountStorage::new(Self::mock_storage_slots()).unwrap()
    }

    pub fn mock_storage_slots() -> Vec<StorageSlot> {
        vec![Self::mock_item_0().slot, Self::mock_item_1().slot, Self::mock_item_2().slot]
    }

    pub fn mock_item_0() -> SlotWithIndex {
        SlotWithIndex {
            slot: StorageSlot::Value(STORAGE_VALUE_0),
            index: STORAGE_INDEX_0,
        }
    }

    pub fn mock_item_1() -> SlotWithIndex {
        SlotWithIndex {
            slot: StorageSlot::Value(STORAGE_VALUE_1),
            index: STORAGE_INDEX_1,
        }
    }

    pub fn mock_item_2() -> SlotWithIndex {
        SlotWithIndex {
            slot: StorageSlot::Map(Self::mock_map()),
            index: STORAGE_INDEX_2,
        }
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
        let asset_word: Word = asset.into();
        let asset_str = word_to_masm_push_string(&asset_word);
        assets.push(asset_str);
    }
    assets
}

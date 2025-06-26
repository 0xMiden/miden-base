use alloc::collections::BTreeMap;

use miden_objects::{Digest, Word, account::AccountStorageHeader};

/// Keeps track of the initial storage of an account during transaction execution.
///
/// For storage value slots this can be simply inspected by looking in to the
/// [`AccountStorageHeader`].
///
/// For map slots, to avoid making a copy of the entire storage map or even requiring that it is
/// fully accessible in the first place, the initial values are tracked lazily. That is, whenever
/// `set_map_item` is called, the previous value is extracted from the stack and if that is the
/// first time the key is written to, then the previous value is the initial value of that key in
/// that slot.
#[derive(Debug, Clone)]
pub struct AccountInitialStorage {
    /// The storage header of the native account against which the transaction is executed. This is
    /// only used to look up the storage value slots.
    header: AccountStorageHeader,
    /// A map from slot index to a map of key-value pairs where the key is a storage map key and
    /// the value represents the value of that key at the beginning of transaction execution.
    maps: BTreeMap<u8, BTreeMap<Digest, Word>>,
}

impl AccountInitialStorage {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Constructs a new initial account storage from a storage header.
    pub fn new(header: AccountStorageHeader) -> Self {
        Self { header, maps: BTreeMap::new() }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a reference to the storage header of the initial storage.
    pub fn storage_header(&self) -> &AccountStorageHeader {
        &self.header
    }

    /// Returns a reference to the storage map at the provided index, if any changes have been made
    /// to the map.
    pub fn init_map(&self, slot_index: u8) -> Option<&BTreeMap<Digest, Word>> {
        self.maps.get(&slot_index)
    }

    // PUBLIC MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Sets the initial value of the given key in the given slot to the given value, if no value is
    /// already tracked for that key.
    pub fn set_init_map_item(&mut self, slot_index: u8, key: Digest, new_value: Word) {
        let slot_map = self.maps.entry(slot_index).or_default();
        slot_map.entry(key).or_insert(new_value);
    }
}

use alloc::collections::BTreeMap;

use miden_objects::Word;
use miden_objects::account::{AccountStorageDelta, AccountStorageHeader};

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
pub struct StorageDeltaTracker {
    /// The _initial_ storage header of the native account against which the transaction is
    /// executed. This is only used to look up the initial values of storage _value_ slots, while
    /// the map slots are unused.
    storage_header: AccountStorageHeader,
    /// A map from slot index to a map of key-value pairs where the key is a storage map key and
    /// the value represents the value of that key at the beginning of transaction execution.
    init_maps: BTreeMap<u8, BTreeMap<Word, Word>>,
    /// The account storage delta.
    delta: AccountStorageDelta,
}

impl StorageDeltaTracker {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Constructs a new initial account storage from a storage header.
    pub fn new(storage_header: AccountStorageHeader) -> Self {
        Self {
            storage_header,
            init_maps: BTreeMap::new(),
            delta: AccountStorageDelta::new(),
        }
    }

    // PUBLIC MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Updates a value slot.
    pub fn set_item(&mut self, slot_index: u8, prev_value: Word, new_value: Word) {
        // Don't update the delta if the new value matches the old one.
        if prev_value != new_value {
            self.delta.set_item(slot_index, new_value);
        }
    }

    /// Updates a map slot.
    pub fn set_map_item(&mut self, slot_index: u8, key: Word, prev_value: Word, new_value: Word) {
        // Don't update the delta if the new value matches the old one.
        if prev_value != new_value {
            self.set_init_map_item(slot_index, key, prev_value);
            self.delta.set_map_item(slot_index, key, new_value);
        }
    }

    /// Consumes `self` and returns the resulting, normalized [`AccountStorageDelta`].
    pub fn into_delta(self) -> AccountStorageDelta {
        self.normalize()
    }

    // HELPERS
    // --------------------------------------------------------------------------------------------

    /// Sets the initial value of the given key in the given slot to the given value, if no value is
    /// already tracked for that key.
    fn set_init_map_item(&mut self, slot_index: u8, key: Word, prev_value: Word) {
        let slot_map = self.init_maps.entry(slot_index).or_default();
        slot_map.entry(key).or_insert(prev_value);
    }

    /// Normalizes the storage delta by:
    ///
    /// - removing entries for value slot updates whose new value is equal to the initial value at
    ///   the beginning of transaction execution.
    /// - removing entries for map slot updates where for a given key, the new value is equal to the
    ///   initial value at the beginning of transaction execution.
    fn normalize(self) -> AccountStorageDelta {
        let Self { storage_header, init_maps, delta } = self;
        let (mut value_slots, mut map_slots) = delta.into_parts();

        // Keep only the values whose new value is different from the initial value.
        value_slots.retain(|slot_idx, new_value| {
            // SAFETY: The header in the initial storage is the one from the account against which
            // the transaction is executed, so accessing that slot index should be fine.
            let (_, _, initial_value) = storage_header
                .slot_header_by_index(*slot_idx as usize)
                .expect("index should be in bounds");
            new_value != initial_value
        });

        // On the key-value level: Keep only the key-value pairs whose new value is different from
        // the initial value.
        // On the map level: Keep only the maps that are non-empty after its key-value pairs have
        // been normalized.
        map_slots.retain(|slot_idx, map_delta| {
            let init_map = init_maps.get(slot_idx);

            if let Some(init_map) = init_map {
                map_delta.as_map_mut().retain(|key, new_value| {
                    let initial_value = init_map.get(key.inner()).expect(
                        "the initial value should be present for every value that was updated",
                    );
                    new_value != initial_value
                });
            }

            // Only retain the map delta if it still contains values after normalization.
            !map_delta.is_empty()
        });

        AccountStorageDelta::from_parts(value_slots, map_slots)
            .expect("storage delta should still be valid since no new values were added")
    }
}

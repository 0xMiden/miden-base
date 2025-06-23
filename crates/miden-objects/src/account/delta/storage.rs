use alloc::{
    collections::{BTreeMap, btree_map::Entry},
    string::ToString,
    vec::Vec,
};

use super::{
    AccountDeltaError, ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable,
    Word,
};
use crate::{
    Digest, EMPTY_WORD, Felt, ZERO,
    account::{AccountStorage, StorageMap, StorageSlot},
};
// ACCOUNT STORAGE DELTA
// ================================================================================================

/// [AccountStorageDelta] stores the differences between two states of account storage.
///
/// The delta consists of two maps:
/// - A map containing the updates to value storage slots. The keys in this map are indexes of the
///   updated storage slots and the values are the new values for these slots.
/// - A map containing updates to storage maps. The keys in this map are indexes of the updated
///   storage slots and the values are corresponding storage map delta objects.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountStorageDelta {
    values: BTreeMap<u8, Word>,
    maps: BTreeMap<u8, StorageMapDelta>,
}

impl AccountStorageDelta {
    /// Creates a new storage delta from the provided fields.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Any of the updated slot is referenced from both maps (e.g., updated twice).
    pub fn new(
        values: BTreeMap<u8, Word>,
        maps: BTreeMap<u8, StorageMapDelta>,
    ) -> Result<Self, AccountDeltaError> {
        let result = Self { values, maps };
        result.validate()?;

        Ok(result)
    }

    /// Returns a reference to the updated values in this storage delta.
    pub fn values(&self) -> &BTreeMap<u8, Word> {
        &self.values
    }

    /// Returns a reference to the updated maps in this storage delta.
    pub fn maps(&self) -> &BTreeMap<u8, StorageMapDelta> {
        &self.maps
    }

    /// Returns true if storage delta contains no updates.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.maps.is_empty()
    }

    /// Tracks a slot change
    pub fn set_item(&mut self, slot_index: u8, new_slot_value: Word) {
        self.values.insert(slot_index, new_slot_value);
    }

    /// Tracks a map item change
    pub fn set_map_item(&mut self, slot_index: u8, key: Digest, new_value: Word) {
        self.maps.entry(slot_index).or_default().insert(key, new_value);
    }

    /// Merges another delta into this one, overwriting any existing values.
    pub fn merge(&mut self, other: Self) -> Result<(), AccountDeltaError> {
        self.values.extend(other.values);

        // merge maps
        for (slot, update) in other.maps.into_iter() {
            match self.maps.entry(slot) {
                Entry::Vacant(entry) => {
                    entry.insert(update);
                },
                Entry::Occupied(mut entry) => entry.get_mut().merge(update),
            }
        }

        self.validate()
    }

    /// Checks whether this storage delta is valid.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Any of the updated slot is referenced from both maps (e.g., updated twice).
    fn validate(&self) -> Result<(), AccountDeltaError> {
        for slot in self.maps.keys() {
            if self.values.contains_key(slot) {
                return Err(AccountDeltaError::StorageSlotUsedAsDifferentTypes(*slot));
            }
        }

        Ok(())
    }

    /// Returns an iterator of all the cleared storage slots.
    fn cleared_slots(&self) -> impl Iterator<Item = u8> + '_ {
        self.values
            .iter()
            .filter(|&(_, value)| (value == &EMPTY_WORD))
            .map(|(slot, _)| *slot)
    }

    /// Returns an iterator of all the updated storage slots.
    fn updated_slots(&self) -> impl Iterator<Item = (&u8, &Word)> + '_ {
        self.values.iter().filter(|&(_, value)| value != &EMPTY_WORD)
    }

    /// Appends the storage slots delta to the given `elements` from which the delta commitment will
    /// be computed.
    ///
    /// TODO: Make num_slots part of this struct.
    pub(super) fn append_delta_elements(&self, elements: &mut Vec<Felt>, num_slots: u8) {
        const DOMAIN_VALUE: Felt = Felt::new(2);
        const DOMAIN_MAP: Felt = Felt::new(3);

        for slot_idx in 0..num_slots {
            let slot_idx_felt = Felt::from(slot_idx);

            // The storage delta ensures that the value slots and map slots do not have overlapping
            // slot indices, so at most one of them will return `Some` for a given slot index.
            match self.values.get(&slot_idx) {
                Some(new_value) => {
                    elements.extend_from_slice(&[DOMAIN_VALUE, slot_idx_felt, ZERO, ZERO]);
                    elements.extend_from_slice(new_value);
                },
                None => {
                    if let Some(map_delta) = self.maps().get(&slot_idx) {
                        if map_delta.is_empty() {
                            continue;
                        }

                        for (key, value) in map_delta.leaves() {
                            elements.extend_from_slice(key.as_elements());
                            elements.extend_from_slice(value);
                        }

                        let num_changed_entries = Felt::try_from(map_delta.num_entries()).expect(
                            "number of changed entries should not exceed max representable felt",
                        );

                        elements.extend_from_slice(&[
                            DOMAIN_MAP,
                            slot_idx_felt,
                            num_changed_entries,
                            ZERO,
                        ]);
                        elements.extend_from_slice(&EMPTY_WORD);
                    }
                },
            }
        }
    }
}

#[cfg(any(feature = "testing", test))]
impl AccountStorageDelta {
    /// Creates an [AccountStorageDelta] from the given iterators.
    pub fn from_iters(
        cleared_items: impl IntoIterator<Item = u8>,
        updated_values: impl IntoIterator<Item = (u8, Word)>,
        updated_maps: impl IntoIterator<Item = (u8, StorageMapDelta)>,
    ) -> Self {
        Self {
            values: BTreeMap::from_iter(
                cleared_items.into_iter().map(|key| (key, EMPTY_WORD)).chain(updated_values),
            ),
            maps: BTreeMap::from_iter(updated_maps),
        }
    }
}

/// Converts an [AccountStorage] into an [AccountStorageDelta] for initial delta construction.
impl From<AccountStorage> for AccountStorageDelta {
    fn from(storage: AccountStorage) -> Self {
        let mut values = BTreeMap::new();
        let mut maps = BTreeMap::new();
        for (slot_idx, slot) in storage.into_iter().enumerate() {
            let slot_idx: u8 = slot_idx.try_into().expect("slot index must fit into `u8`");
            match slot {
                StorageSlot::Value(value) => {
                    values.insert(slot_idx, value);
                },
                StorageSlot::Map(map) => {
                    maps.insert(slot_idx, map.into());
                },
            }
        }

        Self { values, maps }
    }
}

impl Serializable for AccountStorageDelta {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        let cleared: Vec<u8> = self.cleared_slots().collect();
        let updated: Vec<(&u8, &Word)> = self.updated_slots().collect();

        target.write_u8(cleared.len() as u8);
        target.write_many(cleared.iter());

        target.write_u8(updated.len() as u8);
        target.write_many(updated.iter());

        target.write_u8(self.maps.len() as u8);
        target.write_many(self.maps.iter());
    }

    fn get_size_hint(&self) -> usize {
        let u8_size = 0u8.get_size_hint();
        let word_size = EMPTY_WORD.get_size_hint();

        let mut storage_map_delta_size = 0;
        for (slot, storage_map_delta) in self.maps.iter() {
            // The serialized size of each entry is the combination of slot (key) and the delta
            // (value).
            storage_map_delta_size += slot.get_size_hint() + storage_map_delta.get_size_hint();
        }

        // Length Prefixes
        u8_size * 3 +
        // Cleared Slots
        self.cleared_slots().count() * u8_size +
        // Updated Slots
        self.updated_slots().count() * (u8_size + word_size) +
        // Storage Map Delta
        storage_map_delta_size
    }
}

impl Deserializable for AccountStorageDelta {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let mut values = BTreeMap::new();

        let num_cleared_items = source.read_u8()? as usize;
        for _ in 0..num_cleared_items {
            let cleared_slot = source.read_u8()?;
            values.insert(cleared_slot, EMPTY_WORD);
        }

        let num_updated_items = source.read_u8()? as usize;
        for _ in 0..num_updated_items {
            let (updated_slot, updated_value) = source.read()?;
            values.insert(updated_slot, updated_value);
        }

        let num_maps = source.read_u8()? as usize;
        let maps = source.read_many::<(u8, StorageMapDelta)>(num_maps)?.into_iter().collect();

        Self::new(values, maps).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// STORAGE MAP DELTA
// ================================================================================================

/// [StorageMapDelta] stores the differences between two states of account storage maps.
///
/// The differences are represented as leaf updates: a map of updated item key ([Digest]) to
/// value ([Word]). For cleared items the value is [EMPTY_WORD].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StorageMapDelta(BTreeMap<Digest, Word>);

impl StorageMapDelta {
    /// Creates a new storage map delta from the provided leaves.
    pub fn new(map: BTreeMap<Digest, Word>) -> Self {
        Self(map)
    }

    /// Returns the number of changed entries in this map delta.
    pub fn num_entries(&self) -> usize {
        self.0.len()
    }

    /// Returns a reference to the updated leaves in this storage map delta.
    pub fn leaves(&self) -> &BTreeMap<Digest, Word> {
        &self.0
    }

    /// Inserts an item into the storage map delta.
    pub fn insert(&mut self, key: Digest, value: Word) {
        self.0.insert(key, value);
    }

    /// Returns true if storage map delta contains no updates.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Merge `other` into this delta, giving precedence to `other`.
    pub fn merge(&mut self, other: Self) {
        // Aggregate the changes into a map such that `other` overwrites self.
        self.0.extend(other.0);
    }

    /// Returns an iterator of all the cleared keys in the storage map.
    fn cleared_keys(&self) -> impl Iterator<Item = &Digest> + '_ {
        self.0.iter().filter(|&(_, value)| value == &EMPTY_WORD).map(|(key, _)| key)
    }

    /// Returns an iterator of all the updated entries in the storage map.
    fn updated_entries(&self) -> impl Iterator<Item = (&Digest, &Word)> + '_ {
        self.0.iter().filter(|&(_, value)| value != &EMPTY_WORD)
    }
}

#[cfg(any(feature = "testing", test))]
impl StorageMapDelta {
    /// Creates a new [StorageMapDelta] from the provided iterators.
    pub fn from_iters(
        cleared_leaves: impl IntoIterator<Item = Word>,
        updated_leaves: impl IntoIterator<Item = (Word, Word)>,
    ) -> Self {
        Self(BTreeMap::from_iter(
            cleared_leaves
                .into_iter()
                .map(|key| (key.into(), EMPTY_WORD))
                .chain(updated_leaves.into_iter().map(|(key, value)| (key.into(), value))),
        ))
    }
}

/// Converts a [StorageMap] into a [StorageMapDelta] for initial delta construction.
impl From<StorageMap> for StorageMapDelta {
    fn from(map: StorageMap) -> Self {
        StorageMapDelta::new(map.into_entries())
    }
}

impl Serializable for StorageMapDelta {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        let cleared: Vec<&Digest> = self.cleared_keys().collect();
        let updated: Vec<(&Digest, &Word)> = self.updated_entries().collect();

        target.write_usize(cleared.len());
        target.write_many(cleared.iter());

        target.write_usize(updated.len());
        target.write_many(updated.iter());
    }

    fn get_size_hint(&self) -> usize {
        let word_size = EMPTY_WORD.get_size_hint();

        let cleared_keys_count = self.cleared_keys().count();
        let updated_entries_count = self.updated_entries().count();

        // Cleared Keys
        cleared_keys_count.get_size_hint() +
        cleared_keys_count * Digest::SERIALIZED_SIZE +

        // Updated Entries
        updated_entries_count.get_size_hint() +
        updated_entries_count * (Digest::SERIALIZED_SIZE + word_size)
    }
}

impl Deserializable for StorageMapDelta {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let mut map = BTreeMap::new();

        let cleared_count = source.read_usize()?;
        for _ in 0..cleared_count {
            let cleared_key = source.read()?;
            map.insert(cleared_key, EMPTY_WORD);
        }

        let updated_count = source.read_usize()?;
        for _ in 0..updated_count {
            let (updated_key, updated_value) = source.read()?;
            map.insert(updated_key, updated_value);
        }

        Ok(Self::new(map))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::{AccountStorageDelta, Deserializable, Serializable};
    use crate::{
        ONE, ZERO, account::StorageMapDelta, testing::storage::AccountStorageDeltaBuilder,
    };

    #[test]
    fn account_storage_delta_validation() {
        let delta = AccountStorageDelta::from_iters(
            [1, 2, 3],
            [(4, [ONE, ONE, ONE, ONE]), (5, [ONE, ONE, ONE, ZERO])],
            [],
        );
        assert!(delta.validate().is_ok());

        let bytes = delta.to_bytes();
        assert_eq!(AccountStorageDelta::read_from_bytes(&bytes), Ok(delta));

        // duplicate across cleared items and maps
        let delta = AccountStorageDelta::from_iters(
            [1, 2, 3],
            [(2, [ONE, ONE, ONE, ONE]), (5, [ONE, ONE, ONE, ZERO])],
            [(1, StorageMapDelta::default())],
        );
        assert!(delta.validate().is_err());

        let bytes = delta.to_bytes();
        assert!(AccountStorageDelta::read_from_bytes(&bytes).is_err());

        // duplicate across updated items and maps
        let delta = AccountStorageDelta::from_iters(
            [1, 3],
            [(2, [ONE, ONE, ONE, ONE]), (5, [ONE, ONE, ONE, ZERO])],
            [(2, StorageMapDelta::default())],
        );
        assert!(delta.validate().is_err());

        let bytes = delta.to_bytes();
        assert!(AccountStorageDelta::read_from_bytes(&bytes).is_err());
    }

    #[test]
    fn test_is_empty() {
        let storage_delta = AccountStorageDelta::default();
        assert!(storage_delta.is_empty());

        let storage_delta = AccountStorageDelta::from_iters([1], [], []);
        assert!(!storage_delta.is_empty());

        let storage_delta = AccountStorageDelta::from_iters([], [(2, [ONE, ONE, ONE, ONE])], []);
        assert!(!storage_delta.is_empty());

        let storage_delta =
            AccountStorageDelta::from_iters([], [], [(3, StorageMapDelta::default())]);
        assert!(!storage_delta.is_empty());
    }

    #[test]
    fn test_serde_account_storage_delta() {
        let storage_delta = AccountStorageDelta::default();
        let serialized = storage_delta.to_bytes();
        let deserialized = AccountStorageDelta::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, storage_delta);

        let storage_delta = AccountStorageDelta::from_iters([1], [], []);
        let serialized = storage_delta.to_bytes();
        let deserialized = AccountStorageDelta::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, storage_delta);

        let storage_delta = AccountStorageDelta::from_iters([], [(2, [ONE, ONE, ONE, ONE])], []);
        let serialized = storage_delta.to_bytes();
        let deserialized = AccountStorageDelta::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, storage_delta);

        let storage_delta =
            AccountStorageDelta::from_iters([], [], [(3, StorageMapDelta::default())]);
        let serialized = storage_delta.to_bytes();
        let deserialized = AccountStorageDelta::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, storage_delta);
    }

    #[test]
    fn test_serde_storage_map_delta() {
        let storage_map_delta = StorageMapDelta::default();
        let serialized = storage_map_delta.to_bytes();
        let deserialized = StorageMapDelta::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, storage_map_delta);

        let storage_map_delta = StorageMapDelta::from_iters([[ONE, ONE, ONE, ONE]], []);
        let serialized = storage_map_delta.to_bytes();
        let deserialized = StorageMapDelta::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, storage_map_delta);

        let storage_map_delta =
            StorageMapDelta::from_iters([], [([ZERO, ZERO, ZERO, ZERO], [ONE, ONE, ONE, ONE])]);
        let serialized = storage_map_delta.to_bytes();
        let deserialized = StorageMapDelta::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, storage_map_delta);
    }

    #[rstest::rstest]
    #[case::some_some(Some(1), Some(2), Some(2))]
    #[case::none_some(None, Some(2), Some(2))]
    #[case::some_none(Some(1), None, None)]
    #[test]
    fn merge_items(#[case] x: Option<u64>, #[case] y: Option<u64>, #[case] expected: Option<u64>) {
        /// Creates a delta containing the item as an update if Some, else with the item cleared.
        fn create_delta(item: Option<u64>) -> AccountStorageDelta {
            const SLOT: u8 = 123;
            let item = item.map(|x| (SLOT, [vm_core::Felt::new(x), ZERO, ZERO, ZERO]));

            AccountStorageDeltaBuilder::default()
                .add_cleared_items(item.is_none().then_some(SLOT))
                .add_updated_values(item)
                .build()
                .unwrap()
        }

        let mut delta_x = create_delta(x);
        let delta_y = create_delta(y);
        let expected = create_delta(expected);

        delta_x.merge(delta_y).unwrap();

        assert_eq!(delta_x, expected);
    }

    #[rstest::rstest]
    #[case::some_some(Some(1), Some(2), Some(2))]
    #[case::none_some(None, Some(2), Some(2))]
    #[case::some_none(Some(1), None, None)]
    #[test]
    fn merge_maps(#[case] x: Option<u64>, #[case] y: Option<u64>, #[case] expected: Option<u64>) {
        fn create_delta(value: Option<u64>) -> StorageMapDelta {
            let key = [vm_core::Felt::new(10), ZERO, ZERO, ZERO];
            match value {
                Some(value) => StorageMapDelta::from_iters(
                    [],
                    [(key, [vm_core::Felt::new(value), ZERO, ZERO, ZERO])],
                ),
                None => StorageMapDelta::from_iters([key], []),
            }
        }

        let mut delta_x = create_delta(x);
        let delta_y = create_delta(y);
        let expected = create_delta(expected);

        delta_x.merge(delta_y);

        assert_eq!(delta_x, expected);
    }
}

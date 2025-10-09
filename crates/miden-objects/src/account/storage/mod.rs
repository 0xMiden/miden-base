use alloc::collections::BTreeMap;
use alloc::string::ToString;
use alloc::vec::Vec;

use super::{
    AccountError,
    AccountStorageDelta,
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Felt,
    Serializable,
    Word,
};
use crate::account::storage::header::StorageSlotHeader;
use crate::account::{AccountComponent, AccountType};
use crate::crypto::SequentialCommit;

mod slot;
pub use slot::{NamedStorageSlot, SlotName, SlotNameId, StorageSlot, StorageSlotType};

mod map;
pub use map::{PartialStorageMap, StorageMap, StorageMapWitness};

mod header;
pub use header::AccountStorageHeader;

mod partial;
pub use partial::PartialStorage;

// ACCOUNT STORAGE
// ================================================================================================

/// Account storage is composed of a variable number of index-addressable [StorageSlot]s up to
/// 255 slots in total.
///
/// Each slot has a type which defines its size and structure. Currently, the following types are
/// supported:
/// - [StorageSlot::Value]: contains a single [Word] of data (i.e., 32 bytes).
/// - [StorageSlot::Map]: contains a [StorageMap] which is a key-value map where both keys and
///   values are [Word]s. The value of a storage slot containing a map is the commitment to the
///   underlying map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStorage {
    slots: Vec<NamedStorageSlot>,
}

impl AccountStorage {
    /// The maximum number of storage slots allowed in an account storage.
    pub const MAX_NUM_STORAGE_SLOTS: usize = 255;

    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// TODO(named_slots): Remove this temporary API.
    ///
    /// Returns a new instance of account storage initialized with the provided items.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The number of [`StorageSlot`]s exceeds 255.
    pub fn new(slots: Vec<StorageSlot>) -> Result<AccountStorage, AccountError> {
        let slots = slots
            .into_iter()
            .enumerate()
            .map(|(idx, slot)| NamedStorageSlot::new(SlotName::new_index(idx), slot))
            .collect();

        Self::new_named(slots)
    }

    /// TODO(named_slots): Rename to new.
    pub fn new_named(mut slots: Vec<NamedStorageSlot>) -> Result<AccountStorage, AccountError> {
        let num_slots = slots.len();

        if num_slots > Self::MAX_NUM_STORAGE_SLOTS {
            return Err(AccountError::StorageTooManySlots(num_slots as u64));
        }

        let mut names = BTreeMap::new();
        for slot in &slots {
            if let Some(name) = names.insert(slot.name_id(), slot.name()) {
                // TODO(named_slots): Return error.
                todo!("error: storage slot name {name} is assigned to more than one slot")
            }
        }

        // Unstable sort is fine because we require all names to be unique.
        slots.sort_unstable();

        Ok(Self { slots })
    }

    /// Creates an [`AccountStorage`] from the provided components' storage slots.
    ///
    /// If the account type is faucet the reserved slot (slot 0) will be initialized.
    /// - For Fungible Faucets the value is [`StorageSlot::empty_value`].
    /// - For Non-Fungible Faucets the value is [`StorageSlot::empty_map`].
    ///
    /// If the storage needs to be initialized with certain values in that slot, those can be added
    /// after construction with the standard set methods for items and maps.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The number of [`StorageSlot`]s of all components exceeds 255.
    pub(super) fn from_components(
        components: &[AccountComponent],
        account_type: AccountType,
    ) -> Result<AccountStorage, AccountError> {
        let mut storage_slots = match account_type {
            AccountType::FungibleFaucet => {
                vec![NamedStorageSlot::new(SlotName::new_index(0), StorageSlot::empty_value())]
            },
            AccountType::NonFungibleFaucet => {
                vec![NamedStorageSlot::new(SlotName::new_index(0), StorageSlot::empty_map())]
            },
            _ => vec![],
        };

        let offset = storage_slots.len();

        for (slot_idx, slot) in components
            .iter()
            .flat_map(|component| component.storage_slots())
            .cloned()
            .enumerate()
        {
            let name = SlotName::new_index(slot_idx + offset);
            storage_slots.push(NamedStorageSlot::new(name, slot));
        }

        Self::new_named(storage_slots)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Converts storage slots of this account storage into a vector of field elements.
    ///
    /// Each storage slot is represented by exactly 8 elements:
    ///
    /// ```text
    /// [[0, slot_type, name_id_suffix, name_id_prefix], SLOT_VALUE]
    /// ```
    pub fn to_elements(&self) -> Vec<Felt> {
        <Self as SequentialCommit>::to_elements(self)
    }

    /// Returns the commitment to the [`AccountStorage`].
    pub fn to_commitment(&self) -> Word {
        <Self as SequentialCommit>::to_commitment(self)
    }

    /// Returns the number of slots in the account's storage.
    pub fn num_slots(&self) -> u8 {
        // SAFETY: The constructors of account storage ensure that the number of slots fits into a
        // u8.
        self.slots.len() as u8
    }

    /// Returns a reference to the storage slots.
    pub fn slots(&self) -> &[NamedStorageSlot] {
        &self.slots
    }

    /// Returns an [AccountStorageHeader] for this account storage.
    pub fn to_header(&self) -> AccountStorageHeader {
        AccountStorageHeader::new(
            self.slots
                .iter()
                .map(|slot| {
                    (
                        slot.name().clone(),
                        slot.storage_slot().slot_type(),
                        slot.storage_slot().value(),
                    )
                })
                .collect(),
        )
    }

    pub fn get(&self, slot_name: &SlotName) -> Option<&NamedStorageSlot> {
        debug_assert!(self.slots.is_sorted());

        let name_id = slot_name.compute_id();
        self.slots
            .binary_search_by_key(&name_id, |named_slot| named_slot.name_id())
            .map(|idx| &self.slots[idx])
            .ok()
    }

    fn get_mut(&mut self, slot_name: &SlotName) -> Option<&mut NamedStorageSlot> {
        let name_id = slot_name.compute_id();
        self.slots
            .binary_search_by_key(&name_id, |named_slot| named_slot.name_id())
            .map(|idx| &mut self.slots[idx])
            .ok()
    }

    /// Returns an item from the storage at the specified index.
    ///
    /// # Errors:
    /// - If the index is out of bounds
    pub fn get_item(&self, index: u8) -> Result<Word, AccountError> {
        let slot_name = SlotName::new_index(index as usize);
        self.get(&slot_name)
            .map(|named_slot| named_slot.storage_slot().value())
            .ok_or_else(|| AccountError::StorageSlotNameNotFound { slot_name: slot_name.clone() })
    }

    /// Returns a map item from a map located in storage at the specified index.
    ///
    /// # Errors:
    /// - If the index is out of bounds
    /// - If the [StorageSlot] is not [StorageSlotType::Map]
    pub fn get_map_item(&self, index: u8, key: Word) -> Result<Word, AccountError> {
        let slot_name = SlotName::new_index(index as usize);
        self.get(&slot_name)
            .ok_or_else(|| AccountError::StorageSlotNameNotFound { slot_name: slot_name.clone() })
            .and_then(|named_slot| match named_slot.storage_slot() {
                StorageSlot::Map(map) => Ok(map.get(&key)),
                _ => Err(AccountError::StorageSlotNotMap(index)),
            })
    }

    // STATE MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Applies the provided delta to this account storage.
    ///
    /// # Errors:
    /// - If the updates violate storage constraints.
    pub(super) fn apply_delta(&mut self, delta: &AccountStorageDelta) -> Result<(), AccountError> {
        let len = self.slots.len() as u8;

        // update storage maps
        for (&idx, map) in delta.maps().iter() {
            let named_slot = self
                .get_mut(&SlotName::new_index(idx as usize))
                .ok_or(AccountError::StorageIndexOutOfBounds { slots_len: len, index: idx })?;

            let storage_map = match named_slot.storage_slot_mut() {
                StorageSlot::Map(map) => map,
                _ => return Err(AccountError::StorageSlotNotMap(idx)),
            };

            storage_map.apply_delta(map)?;
        }

        // update storage values
        for (&idx, &value) in delta.values().iter() {
            self.set_item(idx, value)?;
        }

        Ok(())
    }

    /// Updates the value of the storage slot at the specified index.
    ///
    /// This method should be used only to update value slots. For updating values
    /// in storage maps, please see [AccountStorage::set_map_item()].
    ///
    /// # Errors:
    /// - If the index is out of bounds
    /// - If the [StorageSlot] is not [StorageSlotType::Value]
    pub fn set_item(&mut self, index: u8, value: Word) -> Result<Word, AccountError> {
        let slot_name = SlotName::new_index(index as usize);
        let slot = self.get_mut(&slot_name).ok_or_else(|| {
            AccountError::StorageSlotNameNotFound { slot_name: slot_name.clone() }
        })?;

        let StorageSlot::Value(old_value) = slot.storage_slot() else {
            return Err(AccountError::StorageSlotNotValue(index));
        };
        let old_value = *old_value;

        let mut new_slot = StorageSlot::Value(value);
        core::mem::swap(slot.storage_slot_mut(), &mut new_slot);

        Ok(old_value)
    }

    /// Updates the value of a key-value pair of a storage map at the specified index.
    ///
    /// This method should be used only to update storage maps. For updating values
    /// in storage slots, please see [AccountStorage::set_item()].
    ///
    /// # Errors:
    /// - If the index is out of bounds
    /// - If the [StorageSlot] is not [StorageSlotType::Map]
    pub fn set_map_item(
        &mut self,
        index: u8,
        raw_key: Word,
        value: Word,
    ) -> Result<(Word, Word), AccountError> {
        let slot_name = SlotName::new_index(index as usize);
        let slot = self.get_mut(&slot_name).ok_or_else(|| {
            AccountError::StorageSlotNameNotFound { slot_name: slot_name.clone() }
        })?;

        let StorageSlot::Map(storage_map) = slot.storage_slot_mut() else {
            return Err(AccountError::StorageSlotNotMap(index));
        };

        let old_root = storage_map.root();

        let old_value = storage_map.insert(raw_key, value)?;

        Ok((old_root, old_value))
    }
}

// ITERATORS
// ================================================================================================

impl IntoIterator for AccountStorage {
    type Item = NamedStorageSlot;
    type IntoIter = alloc::vec::IntoIter<NamedStorageSlot>;

    fn into_iter(self) -> Self::IntoIter {
        self.slots.into_iter()
    }
}

// SEQUENTIAL COMMIT
// ================================================================================================

impl SequentialCommit for AccountStorage {
    type Commitment = Word;

    fn to_elements(&self) -> Vec<Felt> {
        self.slots()
            .iter()
            .flat_map(|named_slot| {
                StorageSlotHeader::new(
                    named_slot.name_id(),
                    named_slot.storage_slot().slot_type(),
                    named_slot.storage_slot().value(),
                )
                .to_elements()
            })
            .collect()
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for AccountStorage {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_u8(self.slots().len() as u8);
        target.write_many(self.slots());
    }

    fn get_size_hint(&self) -> usize {
        // Size of the serialized slot length.
        let u8_size = 0u8.get_size_hint();
        let mut size = u8_size;

        for slot in self.slots() {
            size += slot.get_size_hint();
        }

        size
    }
}

impl Deserializable for AccountStorage {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let num_slots = source.read_u8()? as usize;
        let slots = source.read_many::<NamedStorageSlot>(num_slots)?;

        Self::new_named(slots).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::{AccountStorage, Deserializable, Serializable, StorageMap, Word};
    use crate::account::{NamedStorageSlot, SlotName, StorageSlot};

    #[test]
    fn test_serde_account_storage() {
        // empty storage
        let storage = AccountStorage::new(vec![]).unwrap();
        let bytes = storage.to_bytes();
        assert_eq!(storage, AccountStorage::read_from_bytes(&bytes).unwrap());

        // storage with values for default types
        let storage = AccountStorage::new(vec![
            StorageSlot::Value(Word::empty()),
            StorageSlot::Map(StorageMap::default()),
        ])
        .unwrap();
        let bytes = storage.to_bytes();
        assert_eq!(storage, AccountStorage::read_from_bytes(&bytes).unwrap());
    }

    #[test]
    fn test_get_slot_by_name() -> anyhow::Result<()> {
        // TODO(named_slots): Use proper names.
        // const COUNTER_SLOT: SlotName = SlotName::from_static_str("miden::test::counter");
        // const MAP_SLOT: SlotName = SlotName::from_static_str("miden::test::map");
        const COUNTER_SLOT: SlotName = SlotName::from_static_str("miden::0");
        const MAP_SLOT: SlotName = SlotName::from_static_str("miden::4");

        let slots = vec![
            NamedStorageSlot::new(COUNTER_SLOT, StorageSlot::empty_value()),
            NamedStorageSlot::new(MAP_SLOT, StorageSlot::empty_map()),
        ];
        let storage = AccountStorage::new_named(slots.clone())?;

        assert_eq!(storage.get(&COUNTER_SLOT).unwrap(), &slots[0]);
        assert_eq!(storage.get(&MAP_SLOT).unwrap(), &slots[1]);

        Ok(())
    }
}

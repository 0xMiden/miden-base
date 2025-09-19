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
    Hasher,
    Serializable,
    Word,
};
use crate::account::{AccountComponent, AccountType};

mod slot;
pub use slot::{StorageSlot, StorageSlotType};

mod map;
pub use map::{PartialStorageMap, StorageMap, StorageMapWitness};

mod header;
pub use header::{AccountStorageHeader, StorageSlotHeader};

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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountStorage {
    slots: Vec<StorageSlot>,
}

impl AccountStorage {
    /// The maximum number of storage slots allowed in an account storage.
    pub const MAX_NUM_STORAGE_SLOTS: usize = 255;

    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a new instance of account storage initialized with the provided items.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The number of [`StorageSlot`]s exceeds 255.
    pub fn new(slots: Vec<StorageSlot>) -> Result<AccountStorage, AccountError> {
        let num_slots = slots.len();

        if num_slots > Self::MAX_NUM_STORAGE_SLOTS {
            return Err(AccountError::StorageTooManySlots(num_slots as u64));
        }

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
            AccountType::FungibleFaucet => vec![StorageSlot::empty_value()],
            AccountType::NonFungibleFaucet => vec![StorageSlot::empty_map()],
            _ => vec![],
        };

        storage_slots
            .extend(components.iter().flat_map(|component| component.storage_slots()).cloned());

        Self::new(storage_slots)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns a commitment to this storage.
    pub fn commitment(&self) -> Word {
        build_slots_commitment(&self.slots)
    }

    /// Returns the number of slots in the account's storage.
    pub fn num_slots(&self) -> u8 {
        // SAFETY: The constructors of account storage ensure that the number of slots fits into a
        // u8.
        self.slots.len() as u8
    }

    /// Returns a reference to the storage slots.
    pub fn slots(&self) -> &[StorageSlot] {
        &self.slots
    }

    /// Consumes self and returns the storage slots of the account storage.
    pub fn into_slots(self) -> Vec<StorageSlot> {
        self.slots
    }

    /// Returns an [AccountStorageHeader] for this account storage.
    pub fn to_header(&self) -> AccountStorageHeader {
        AccountStorageHeader::new(
            self.slots.iter().map(|slot| (slot.slot_type(), slot.value())).collect(),
        )
    }

    /// Returns an item from the storage at the specified index.
    ///
    /// # Errors:
    /// - If the index is out of bounds
    pub fn get_item(&self, index: u8) -> Result<Word, AccountError> {
        self.slots
            .get(index as usize)
            .ok_or(AccountError::StorageIndexOutOfBounds {
                slots_len: self.slots.len() as u8,
                index,
            })
            .map(|slot| slot.value())
    }

    /// Returns a map item from a map located in storage at the specified index.
    ///
    /// # Errors:
    /// - If the index is out of bounds
    /// - If the [StorageSlot] is not [StorageSlotType::Map]
    pub fn get_map_item(&self, index: u8, key: Word) -> Result<Word, AccountError> {
        match self.slots.get(index as usize).ok_or(AccountError::StorageIndexOutOfBounds {
            slots_len: self.slots.len() as u8,
            index,
        })? {
            StorageSlot::Map(map) => Ok(map.get(&key)),
            _ => Err(AccountError::StorageSlotNotMap(index)),
        }
    }

    /// Converts storage slots of this account storage into a vector of field elements.
    ///
    /// This is done by first converting each storage slot into exactly 8 elements as follows:
    /// ```text
    /// [STORAGE_SLOT_VALUE, storage_slot_type, 0, 0, 0]
    /// ```
    /// And then concatenating the resulting elements into a single vector.
    pub fn as_elements(&self) -> Vec<Felt> {
        slots_as_elements(self.slots())
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
            let storage_slot = self
                .slots
                .get_mut(idx as usize)
                .ok_or(AccountError::StorageIndexOutOfBounds { slots_len: len, index: idx })?;

            let storage_map = match storage_slot {
                StorageSlot::Map(map) => map,
                _ => return Err(AccountError::StorageSlotNotMap(idx)),
            };

            storage_map.apply_delta(map);
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
        // check if index is in bounds
        let num_slots = self.slots.len();

        if index as usize >= num_slots {
            return Err(AccountError::StorageIndexOutOfBounds {
                slots_len: self.slots.len() as u8,
                index,
            });
        }

        let old_value = match self.slots[index as usize] {
            StorageSlot::Value(value) => value,
            // return an error if the type != Value
            _ => return Err(AccountError::StorageSlotNotValue(index)),
        };

        // update the value of the storage slot
        self.slots[index as usize] = StorageSlot::Value(value);

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
        key: Word,
        value: Word,
    ) -> Result<(Word, Word), AccountError> {
        // check if index is in bounds
        let num_slots = self.slots.len();

        if index as usize >= num_slots {
            return Err(AccountError::StorageIndexOutOfBounds {
                slots_len: self.slots.len() as u8,
                index,
            });
        }

        let storage_map = match self.slots[index as usize] {
            StorageSlot::Map(ref mut map) => map,
            _ => return Err(AccountError::StorageSlotNotMap(index)),
        };

        // get old map root to return
        let old_root = storage_map.root();

        // update the key-value pair in the map
        let old_value = storage_map.insert(key, value);

        Ok((old_root, old_value))
    }
}

// ITERATORS
// ================================================================================================

impl IntoIterator for AccountStorage {
    type Item = StorageSlot;
    type IntoIter = alloc::vec::IntoIter<StorageSlot>;

    fn into_iter(self) -> Self::IntoIter {
        self.slots.into_iter()
    }
}

// HELPER FUNCTIONS
// ------------------------------------------------------------------------------------------------

/// Converts given slots into field elements
fn slots_as_elements(slots: &[StorageSlot]) -> Vec<Felt> {
    slots
        .iter()
        .flat_map(|slot| StorageSlotHeader::from(slot).as_elements())
        .collect()
}

/// Computes the commitment to the given slots
pub fn build_slots_commitment(slots: &[StorageSlot]) -> Word {
    let elements = slots_as_elements(slots);
    Hasher::hash_elements(&elements)
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
        let slots = source.read_many::<StorageSlot>(num_slots)?;

        Self::new(slots).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::{
        AccountStorage,
        Deserializable,
        Serializable,
        StorageMap,
        Word,
        build_slots_commitment,
    };
    use crate::account::StorageSlot;

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
    fn test_account_storage_slots_commitment() {
        let storage = AccountStorage::mock();
        let storage_slots_commitment = build_slots_commitment(storage.slots());
        assert_eq!(storage_slots_commitment, storage.commitment())
    }
}

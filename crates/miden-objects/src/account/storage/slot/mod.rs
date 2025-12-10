use miden_core::EMPTY_WORD;
use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_processor::DeserializationError;

use super::map::EMPTY_STORAGE_MAP_ROOT;
use super::{StorageMap, Word};

mod slot_name;
pub use slot_name::StorageSlotName;

mod slot_id;
pub use slot_id::StorageSlotId;

mod r#type;
pub use r#type::StorageSlotType;

mod named_slot;
pub use named_slot::NamedStorageSlot;

// STORAGE SLOT
// ================================================================================================

/// An object representing the contents of an account's storage slot.
///
/// An account storage slot can be of two types:
/// - A simple value which contains a single word (4 field elements or ~32 bytes).
/// - A key value map where both keys and values are words. The capacity of such storage slot is
///   theoretically unlimited.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageSlot {
    Value(Word),
    Map(StorageMap),
}

impl StorageSlot {
    /// The number of field elements needed to represent a [StorageSlot] in kernel memory.
    pub const NUM_ELEMENTS_PER_STORAGE_SLOT: usize = 8;

    /// Returns true if this storage slot has a value equal the default of it's type
    pub fn is_default(&self) -> bool {
        match self {
            StorageSlot::Value(value) => *value == EMPTY_WORD,
            StorageSlot::Map(map) => map.root() == EMPTY_STORAGE_MAP_ROOT,
        }
    }

    /// Returns the empty [Word] for a storage slot of this type
    pub fn default_word(&self) -> Word {
        match self {
            StorageSlot::Value(_) => EMPTY_WORD,
            StorageSlot::Map(_) => EMPTY_STORAGE_MAP_ROOT,
        }
    }

    /// Returns a [`StorageSlot::Value`] with an empty word.
    pub fn empty_value() -> Self {
        StorageSlot::Value(EMPTY_WORD)
    }

    /// Returns an empty [`StorageSlot::Map`].
    pub fn empty_map() -> Self {
        StorageSlot::Map(StorageMap::new())
    }

    /// Returns this storage slot value as a [Word]
    ///
    /// Returns:
    /// - For [`StorageSlot::Value`] the value.
    /// - For [`StorageSlot::Map`] the root of the [StorageMap].
    pub fn value(&self) -> Word {
        match self {
            Self::Value(value) => *value,
            Self::Map(map) => map.root(),
        }
    }

    /// Returns the type of this storage slot
    pub fn slot_type(&self) -> StorageSlotType {
        match self {
            StorageSlot::Value(_) => StorageSlotType::Value,
            StorageSlot::Map(_) => StorageSlotType::Map,
        }
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for StorageSlot {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(self.slot_type());

        match self {
            Self::Value(value) => target.write(value),
            Self::Map(map) => target.write(map),
        }
    }

    fn get_size_hint(&self) -> usize {
        let mut size = self.slot_type().get_size_hint();

        size += match self {
            StorageSlot::Value(word) => word.get_size_hint(),
            StorageSlot::Map(storage_map) => storage_map.get_size_hint(),
        };

        size
    }
}

impl Deserializable for StorageSlot {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let storage_slot_type = source.read::<StorageSlotType>()?;

        match storage_slot_type {
            StorageSlotType::Value => {
                let word = source.read::<Word>()?;
                Ok(StorageSlot::Value(word))
            },
            StorageSlotType::Map => {
                let map = source.read::<StorageMap>()?;
                Ok(StorageSlot::Map(map))
            },
        }
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use miden_core::utils::{Deserializable, Serializable};

    use crate::account::AccountStorage;

    #[test]
    fn test_serde_account_storage_slot() {
        let storage = AccountStorage::mock();
        let serialized = storage.to_bytes();
        let deserialized = AccountStorage::read_from_bytes(&serialized).unwrap();
        assert_eq!(deserialized, storage)
    }
}

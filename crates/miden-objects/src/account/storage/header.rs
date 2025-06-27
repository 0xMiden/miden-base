use alloc::vec::Vec;

use vm_processor::Digest;

use super::{AccountStorage, Felt, Hasher, StorageSlot, StorageSlotType, Word};
use crate::{
    AccountError, ZERO,
    utils::serde::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable},
};

// ACCOUNT STORAGE HEADER
// ================================================================================================

/// Storage slot header is a lighter version of the [StorageSlot] storing only the type and the
/// top-level value for the slot, and being, in fact, just a thin wrapper around a tuple.
///
/// That is, for complex storage slot (e.g., storage map), the header contains only the commitment
/// to the underlying data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageSlotHeader(StorageSlotType, Word);

impl StorageSlotHeader {
    /// Returns a new instance of storage slot header from the provided storage slot type and value.
    pub fn new(value: &(StorageSlotType, Word)) -> Self {
        Self(value.0, value.1)
    }

    /// Returns this storage slot header as field elements.
    ///
    /// This is done by converting this storage slot into 8 field elements as follows:
    /// ```text
    /// [SLOT_VALUE, slot_type, 0, 0, 0]
    /// ```
    pub fn as_elements(&self) -> [Felt; StorageSlot::NUM_ELEMENTS_PER_STORAGE_SLOT] {
        let mut elements = [ZERO; StorageSlot::NUM_ELEMENTS_PER_STORAGE_SLOT];
        elements[0..4].copy_from_slice(&self.1);
        elements[4..8].copy_from_slice(&self.0.as_word());
        elements
    }
}

impl From<&StorageSlot> for StorageSlotHeader {
    fn from(value: &StorageSlot) -> Self {
        Self(value.slot_type(), value.value())
    }
}

/// Account storage header is a lighter version of the [AccountStorage] storing only the type and
/// the top-level value for each storage slot.
///
/// That is, for complex storage slots (e.g., storage maps), the header contains only the commitment
/// to the underlying data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStorageHeader {
    slots: Vec<(StorageSlotType, Word)>,
}

impl AccountStorageHeader {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a new instance of account storage header initialized with the provided slots.
    ///
    /// # Panics
    /// - If the number of provided slots is greater than [AccountStorage::MAX_NUM_STORAGE_SLOTS].
    pub fn new(slots: Vec<(StorageSlotType, Word)>) -> Self {
        assert!(slots.len() <= AccountStorage::MAX_NUM_STORAGE_SLOTS);
        Self { slots }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns an iterator over the storage header slots.
    pub fn slots(&self) -> impl Iterator<Item = &(StorageSlotType, Word)> {
        self.slots.iter()
    }

    /// Returns the number of slots contained in the storage header.
    pub fn num_slots(&self) -> u8 {
        // SAFETY: The constructors of this type ensure this value fits in a u8.
        self.slots.len() as u8
    }

    /// Returns a slot contained in the storage header at a given index.
    ///
    /// # Errors
    /// - If the index is out of bounds.
    pub fn slot(&self, index: usize) -> Result<&(StorageSlotType, Word), AccountError> {
        self.slots.get(index).ok_or(AccountError::StorageIndexOutOfBounds {
            slots_len: self.slots.len() as u8,
            index: index as u8,
        })
    }

    // NOTE: The way of computing the commitment should be kept in sync with `AccountStorage`
    /// Computes the account storage header commitment.
    pub fn compute_commitment(&self) -> Digest {
        Hasher::hash_elements(&self.as_elements())
    }

    /// Converts storage slots of this account storage header into a vector of field elements.
    ///
    /// This is done by first converting each storage slot into exactly 8 elements as follows:
    /// ```text
    /// [STORAGE_SLOT_VALUE, storage_slot_type, 0, 0, 0]
    /// ```
    /// And then concatenating the resulting elements into a single vector.
    pub fn as_elements(&self) -> Vec<Felt> {
        self.slots
            .iter()
            .flat_map(|slot| StorageSlotHeader::new(slot).as_elements())
            .collect()
    }
}

impl From<&AccountStorage> for AccountStorageHeader {
    fn from(value: &AccountStorage) -> Self {
        value.to_header()
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for AccountStorageHeader {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        let len = self.slots.len() as u8;
        target.write_u8(len);
        target.write_many(self.slots())
    }
}

impl Deserializable for AccountStorageHeader {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let len = source.read_u8()?;
        let slots = source.read_many(len as usize)?;
        // number of storage slots is guaranteed to be smaller than or equal to 255
        Ok(Self::new(slots))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use vm_core::{
        Felt,
        utils::{Deserializable, Serializable},
    };

    use super::AccountStorageHeader;
    use crate::account::{AccountStorage, StorageSlotType};

    #[test]
    fn test_from_account_storage() {
        let storage_map = AccountStorage::mock_map();

        // create new storage header from AccountStorage
        let slots = vec![
            (StorageSlotType::Value, [Felt::new(1), Felt::new(2), Felt::new(3), Felt::new(4)]),
            (StorageSlotType::Value, [Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)]),
            (StorageSlotType::Map, storage_map.root().into()),
        ];

        let expected_header = AccountStorageHeader { slots };
        let account_storage = AccountStorage::mock();

        assert_eq!(expected_header, AccountStorageHeader::from(&account_storage))
    }

    #[test]
    fn test_serde_account_storage_header() {
        // create new storage header
        let storage = AccountStorage::mock();
        let storage_header = AccountStorageHeader::from(&storage);

        // serde storage header
        let bytes = storage_header.to_bytes();
        let deserialized = AccountStorageHeader::read_from_bytes(&bytes).unwrap();

        // assert deserialized == storage header
        assert_eq!(storage_header, deserialized);
    }
}

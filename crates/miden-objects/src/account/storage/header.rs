use alloc::string::ToString;
use alloc::vec::Vec;

use super::{AccountStorage, Felt, StorageSlot, StorageSlotType, Word};
use crate::account::{SlotName, StorageSlotId};
use crate::crypto::SequentialCommit;
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{AccountError, FieldElement, ZERO};

// ACCOUNT STORAGE HEADER
// ================================================================================================

/// The header of a [`StorageSlot`], storing only the slot ID, slot type and value of the slot.
///
/// The stored value differs based on the slot type:
/// - [`StorageSlotType::Value`]: The value of the slot itself.
/// - [`StorageSlotType::Map`]: The root of the SMT that represents the storage map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StorageSlotHeader {
    id: StorageSlotId,
    r#type: StorageSlotType,
    value: Word,
}

impl StorageSlotHeader {
    /// Returns a new instance of storage slot header from the provided storage slot type and value.
    pub(crate) fn new(id: StorageSlotId, r#type: StorageSlotType, value: Word) -> Self {
        Self { id, r#type, value }
    }

    /// Returns this storage slot header as field elements.
    ///
    /// This is done by converting this storage slot into 8 field elements as follows:
    /// ```text
    /// [[0, slot_type, slot_id_suffix, slot_id_prefix], SLOT_VALUE]
    /// ```
    pub(crate) fn to_elements(&self) -> [Felt; StorageSlot::NUM_ELEMENTS_PER_STORAGE_SLOT] {
        let mut elements = [ZERO; StorageSlot::NUM_ELEMENTS_PER_STORAGE_SLOT];
        elements[0..4].copy_from_slice(&[
            Felt::ZERO,
            self.r#type.as_felt(),
            self.id.suffix(),
            self.id.prefix(),
        ]);
        elements[4..8].copy_from_slice(self.value.as_elements());
        elements
    }
}

/// The header of an [`AccountStorage`], storing only the slot name, slot type and value of each
/// storage slot.
///
/// The stored value differs based on the slot type:
/// - [`StorageSlotType::Value`]: The value of the slot itself.
/// - [`StorageSlotType::Map`]: The root of the SMT that represents the storage map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountStorageHeader {
    slots: Vec<(SlotName, StorageSlotType, Word)>,
}

impl AccountStorageHeader {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Returns a new instance of account storage header initialized with the provided slots.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The number of provided slots is greater than [`AccountStorage::MAX_NUM_STORAGE_SLOTS`].
    /// - The slots are not sorted by [`StorageSlotId`].
    pub fn new(slots: Vec<(SlotName, StorageSlotType, Word)>) -> Result<Self, AccountError> {
        if slots.len() > AccountStorage::MAX_NUM_STORAGE_SLOTS {
            return Err(AccountError::StorageTooManySlots(slots.len() as u64));
        }

        if !slots.is_sorted_by_key(|(slot_name, ..)| slot_name.compute_id()) {
            return Err(AccountError::UnsortedStorageSlots);
        }

        Ok(Self { slots })
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns an iterator over the storage header slots.
    pub fn slots(&self) -> impl Iterator<Item = (&SlotName, &StorageSlotType, &Word)> {
        self.slots.iter().map(|(name, r#type, value)| (name, r#type, value))
    }

    /// Returns an iterator over the storage header map slots.
    pub fn map_slot_roots(&self) -> impl Iterator<Item = Word> {
        self.slots.iter().filter_map(|(_, slot_type, value)| match slot_type {
            StorageSlotType::Value => None,
            StorageSlotType::Map => Some(*value),
        })
    }

    /// Returns the number of slots contained in the storage header.
    pub fn num_slots(&self) -> u8 {
        // SAFETY: The constructors of this type ensure this value fits in a u8.
        self.slots.len() as u8
    }

    /// Returns a slot contained in the storage header at a given index.
    ///
    /// Returns `None` if a slot with the provided slot ID does not exist.
    pub fn find_slot_header_by_name(
        &self,
        slot_name: &SlotName,
    ) -> Option<(&StorageSlotType, &Word)> {
        self.find_slot_header_by_id(slot_name.compute_id())
            .map(|(_slot_name, slot_type, slot_value)| (slot_type, slot_value))
    }

    /// Returns a slot contained in the storage header at a given index.
    ///
    /// Returns `None` if a slot with the provided slot ID does not exist.
    pub fn find_slot_header_by_id(
        &self,
        slot_id: StorageSlotId,
    ) -> Option<(&SlotName, &StorageSlotType, &Word)> {
        self.slots
            .binary_search_by_key(&slot_id, |(name, ..)| name.compute_id())
            .map(|slot_idx| {
                let (name, r#type, value) = &self.slots[slot_idx];
                (name, r#type, value)
            })
            .ok()
    }

    /// Indicates whether the slot with the given `name` is a map slot.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - a slot with the provided name does not exist.
    pub fn is_map_slot(&self, name: &SlotName) -> Result<bool, AccountError> {
        match self
            .find_slot_header_by_name(name)
            .ok_or(AccountError::StorageSlotNameNotFound { slot_name: name.clone() })?
            .0
        {
            StorageSlotType::Map => Ok(true),
            StorageSlotType::Value => Ok(false),
        }
    }

    /// Converts storage slots of this account storage header into a vector of field elements.
    ///
    /// This is done by first converting each storage slot into exactly 8 elements as follows:
    ///
    /// ```text
    /// [[0, slot_type, slot_id_suffix, slot_id_prefix], SLOT_VALUE]
    /// ```
    ///
    /// And then concatenating the resulting elements into a single vector.
    pub fn to_elements(&self) -> Vec<Felt> {
        <Self as SequentialCommit>::to_elements(self)
    }

    /// Returns the commitment to the [`AccountStorage`] this header represents.
    pub fn to_commitment(&self) -> Word {
        <Self as SequentialCommit>::to_commitment(self)
    }
}

impl From<&AccountStorage> for AccountStorageHeader {
    fn from(value: &AccountStorage) -> Self {
        value.to_header()
    }
}

// SEQUENTIAL COMMIT
// ================================================================================================

impl SequentialCommit for AccountStorageHeader {
    type Commitment = Word;

    fn to_elements(&self) -> Vec<Felt> {
        self.slots()
            .flat_map(|(slot_name, slot_type, slot_value)| {
                StorageSlotHeader::new(slot_name.compute_id(), *slot_type, *slot_value)
                    .to_elements()
            })
            .collect()
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
        Self::new(slots).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use miden_core::Felt;
    use miden_core::utils::{Deserializable, Serializable};

    use super::AccountStorageHeader;
    use crate::Word;
    use crate::account::{AccountStorage, StorageSlotType};
    use crate::testing::storage::{MOCK_MAP_SLOT, MOCK_VALUE_SLOT0, MOCK_VALUE_SLOT1};

    #[test]
    fn test_from_account_storage() {
        let storage_map = AccountStorage::mock_map();

        // create new storage header from AccountStorage
        let mut slots = vec![
            (MOCK_VALUE_SLOT0.clone(), StorageSlotType::Value, Word::from([1, 2, 3, 4u32])),
            (
                MOCK_VALUE_SLOT1.clone(),
                StorageSlotType::Value,
                Word::from([Felt::new(5), Felt::new(6), Felt::new(7), Felt::new(8)]),
            ),
            (MOCK_MAP_SLOT.clone(), StorageSlotType::Map, storage_map.root()),
        ];
        slots.sort_unstable_by_key(|(slot_name, ..)| slot_name.compute_id());

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

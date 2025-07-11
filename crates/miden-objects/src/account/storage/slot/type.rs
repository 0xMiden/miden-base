use alloc::string::{String, ToString};

use crate::{
    Felt, Word,
    utils::serde::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable},
};

// STORAGE SLOT TYPE
// ================================================================================================

/// An object that represents the type of a storage slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageSlotType {
    /// Represents a slot that contains a value.
    Value,
    /// Represents a slot that contains a commitment to a map with key-value pairs.
    Map,
}

impl StorageSlotType {
    /// Returns storage slot type as a [Word]
    pub fn as_word(&self) -> Word {
        match self {
            StorageSlotType::Value => Word::empty(),
            StorageSlotType::Map => Word::from([1, 0, 0, 0u32]),
        }
    }
}

impl TryFrom<Felt> for StorageSlotType {
    type Error = String;

    fn try_from(value: Felt) -> Result<Self, Self::Error> {
        let value = value.as_int();

        match value {
            0 => Ok(StorageSlotType::Value),
            1 => Ok(StorageSlotType::Map),
            _ => Err("No storage slot type exists for this field element.".to_string()),
        }
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for StorageSlotType {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        match self {
            Self::Value => target.write_u8(0),
            Self::Map => target.write_u8(1),
        }
    }

    fn get_size_hint(&self) -> usize {
        // The serialized size of a slot type.
        0u8.get_size_hint()
    }
}

impl Deserializable for StorageSlotType {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let storage_slot_type = source.read_u8()?;

        match storage_slot_type {
            0 => Ok(Self::Value),
            1 => Ok(Self::Map),
            _ => Err(DeserializationError::InvalidValue(storage_slot_type.to_string())),
        }
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use vm_core::utils::{Deserializable, Serializable};

    use crate::account::StorageSlotType;

    #[test]
    fn test_serde_account_storage_slot_type() {
        let type_0 = StorageSlotType::Value;
        let type_1 = StorageSlotType::Value;
        let type_0_bytes = type_0.to_bytes();
        let type_1_bytes = type_1.to_bytes();
        let deserialized_0 = StorageSlotType::read_from_bytes(&type_0_bytes).unwrap();
        let deserialized_1 = StorageSlotType::read_from_bytes(&type_1_bytes).unwrap();
        assert_eq!(type_0, deserialized_0);
        assert_eq!(type_1, deserialized_1);
    }
}

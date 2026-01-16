use alloc::collections::BTreeMap;

use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_processor::DeserializationError;

use super::type_registry::SchemaRequirement;
use super::{InitStorageData, MapSlotSchema, StorageValueName, ValueSlotSchema};
use crate::account::{StorageSlot, StorageSlotName};
use crate::errors::AccountComponentTemplateError;

// STORAGE SLOT SCHEMA
// ================================================================================================

/// Describes the schema for a storage slot.
/// Can describe either a value slot, or a map slot.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageSlotSchema {
    Value(ValueSlotSchema),
    Map(MapSlotSchema),
}

impl StorageSlotSchema {
    pub(super) fn collect_init_value_requirements(
        &self,
        slot_name: &StorageSlotName,
        requirements: &mut BTreeMap<StorageValueName, SchemaRequirement>,
    ) -> Result<(), AccountComponentTemplateError> {
        let slot_name = StorageValueName::from_slot_name(slot_name);
        match self {
            StorageSlotSchema::Value(slot) => {
                slot.collect_init_value_requirements(slot_name, requirements)
            },
            StorageSlotSchema::Map(_) => Ok(()),
        }
    }

    /// Builds a [`StorageSlot`] for the specified `slot_name` using the provided initialization
    /// data.
    pub fn try_build_storage_slot(
        &self,
        slot_name: &StorageSlotName,
        init_storage_data: &InitStorageData,
    ) -> Result<StorageSlot, AccountComponentTemplateError> {
        match self {
            StorageSlotSchema::Value(slot) => {
                let word = slot.try_build_word(init_storage_data, slot_name)?;
                Ok(StorageSlot::with_value(slot_name.clone(), word))
            },
            StorageSlotSchema::Map(slot) => {
                let storage_map = slot.try_build_map(init_storage_data, slot_name)?;
                Ok(StorageSlot::with_map(slot_name.clone(), storage_map))
            },
        }
    }

    pub(crate) fn validate(
        &self,
        slot_name: &StorageSlotName,
    ) -> Result<(), AccountComponentTemplateError> {
        match self {
            StorageSlotSchema::Value(slot) => slot.validate(slot_name)?,
            StorageSlotSchema::Map(slot) => slot.validate()?,
        }

        Ok(())
    }
}

impl Serializable for StorageSlotSchema {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        match self {
            StorageSlotSchema::Value(slot) => {
                target.write_u8(0u8);
                slot.write_into(target);
            },
            StorageSlotSchema::Map(slot) => {
                target.write_u8(1u8);
                slot.write_into(target);
            },
        }
    }
}

impl Deserializable for StorageSlotSchema {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let variant_tag = source.read_u8()?;
        match variant_tag {
            0 => Ok(StorageSlotSchema::Value(ValueSlotSchema::read_from(source)?)),
            1 => Ok(StorageSlotSchema::Map(MapSlotSchema::read_from(source)?)),
            _ => Err(DeserializationError::InvalidValue(format!(
                "unknown variant tag '{variant_tag}' for StorageSlotSchema"
            ))),
        }
    }
}

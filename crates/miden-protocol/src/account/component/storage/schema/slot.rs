use alloc::collections::BTreeMap;
use alloc::string::String;

use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_processor::DeserializationError;

use super::super::type_registry::SchemaRequirement;
use super::super::{InitStorageData, StorageValueName};
use super::{FeltSchema, MapSlotSchema, ValueSlotSchema, WordSchema};
use crate::account::{StorageSlot, StorageSlotName, StorageSlotType};
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
    // CONVENIENCE CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a value slot schema from an array of [`FeltSchema`]s.
    ///
    /// This is a shorthand for creating a value slot with a composite word schema.
    ///
    /// # Example
    /// ```ignore
    /// StorageSlotSchema::value([
    ///     FeltSchema::new_typed("felt", "max_supply").with_description("Maximum token supply"),
    ///     FeltSchema::new_typed("u8", "decimals").with_description("Number of decimals"),
    ///     FeltSchema::new_typed("token_symbol", "symbol"),
    ///     FeltSchema::new_void(),
    /// ])
    /// ```
    pub fn value(elements: [FeltSchema; 4]) -> Self {
        StorageSlotSchema::Value(ValueSlotSchema::new(None, WordSchema::new_value(elements)))
    }

    /// Creates a map slot schema with word keys and word values.
    pub fn map() -> Self {
        StorageSlotSchema::Map(MapSlotSchema::new(
            None,
            None,
            WordSchema::word(),
            WordSchema::word(),
        ))
    }

    /// Creates a value slot schema from a [`WordSchema`].
    ///
    /// This is useful for simple (non-composite) word types.
    ///
    /// # Example
    /// ```ignore
    /// StorageSlotSchema::typed_value(WordSchema::falcon512_rpo_pubkey())
    ///     .with_description("Falcon512 RPO public key")
    /// ```
    pub fn typed_value(word_schema: WordSchema) -> Self {
        StorageSlotSchema::Value(ValueSlotSchema::new(None, word_schema))
    }

    /// Sets the description of this slot schema and returns `self`.
    pub fn with_description(self, description: impl Into<String>) -> Self {
        match self {
            StorageSlotSchema::Value(slot) => StorageSlotSchema::Value(ValueSlotSchema::new(
                Some(description.into()),
                slot.word().clone(),
            )),
            StorageSlotSchema::Map(slot) => StorageSlotSchema::Map(MapSlotSchema::new(
                Some(description.into()),
                slot.default_values(),
                slot.key_schema().clone(),
                slot.value_schema().clone(),
            )),
        }
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`StorageSlotType`] that this schema describes.
    pub fn slot_type(&self) -> StorageSlotType {
        match self {
            StorageSlotSchema::Value(_) => StorageSlotType::Value,
            StorageSlotSchema::Map(_) => StorageSlotType::Map,
        }
    }

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

    pub(super) fn validate(&self) -> Result<(), AccountComponentTemplateError> {
        match self {
            StorageSlotSchema::Value(slot) => slot.validate()?,
            StorageSlotSchema::Map(slot) => slot.validate()?,
        }

        Ok(())
    }

    pub(super) fn write_into_with_optional_defaults<W: ByteWriter>(
        &self,
        target: &mut W,
        include_defaults: bool,
    ) {
        match self {
            StorageSlotSchema::Value(slot) => {
                target.write_u8(0u8);
                slot.write_into_with_optional_defaults(target, include_defaults);
            },
            StorageSlotSchema::Map(slot) => {
                target.write_u8(1u8);
                slot.write_into_with_optional_defaults(target, include_defaults);
            },
        }
    }
}

impl Serializable for StorageSlotSchema {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.write_into_with_optional_defaults(target, true);
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

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_processor::DeserializationError;

use super::super::type_registry::{SCHEMA_TYPE_REGISTRY, SchemaTypeId};
use super::super::{InitStorageData, StorageValueName, WordValue};
use super::{FeltSchema, WordSchema, validate_description_ascii};
use crate::account::{StorageMap, StorageSlotName};
use crate::errors::AccountComponentTemplateError;
use crate::{Felt, FieldElement, Word};

// MAP SLOT SCHEMA
// ================================================================================================

/// Describes the schema for a storage map slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSlotSchema {
    description: Option<String>,
    default_values: Option<BTreeMap<Word, Word>>,
    key_schema: WordSchema,
    value_schema: WordSchema,
}

impl MapSlotSchema {
    pub fn new(
        description: Option<String>,
        default_values: Option<BTreeMap<Word, Word>>,
        key_schema: WordSchema,
        value_schema: WordSchema,
    ) -> Self {
        Self {
            description,
            default_values,
            key_schema,
            value_schema,
        }
    }

    pub fn description(&self) -> Option<&String> {
        self.description.as_ref()
    }

    /// Builds a [`StorageMap`] from the provided initialization data.
    ///
    /// Merges any default values with entries from the init data, validating that the data
    /// contains map entries (not a direct value or field entries).
    pub fn try_build_map(
        &self,
        init_storage_data: &InitStorageData,
        slot_name: &StorageSlotName,
    ) -> Result<StorageMap, AccountComponentTemplateError> {
        let mut entries = self.default_values.clone().unwrap_or_default();
        let slot_prefix = StorageValueName::from_slot_name(slot_name);

        if init_storage_data.slot_value_entry(slot_name).is_some() {
            return Err(AccountComponentTemplateError::InvalidInitStorageValue(
                slot_prefix,
                "expected a map, got a value".into(),
            ));
        }
        if init_storage_data.has_field_entries_for_slot(slot_name) {
            return Err(AccountComponentTemplateError::InvalidInitStorageValue(
                slot_prefix,
                "expected a map, got field entries".into(),
            ));
        }
        if let Some(init_entries) = init_storage_data.map_entries(slot_name) {
            let mut parsed_entries = Vec::with_capacity(init_entries.len());
            for (raw_key, raw_value) in init_entries.iter() {
                let key = parse_storage_value_with_schema(&self.key_schema, raw_key, &slot_prefix)?;
                let value =
                    parse_storage_value_with_schema(&self.value_schema, raw_value, &slot_prefix)?;

                parsed_entries.push((key, value));
            }

            for (key, value) in parsed_entries.iter() {
                entries.insert(*key, *value);
            }
        }

        if entries.is_empty() {
            return Ok(StorageMap::new());
        }

        StorageMap::with_entries(entries)
            .map_err(|err| AccountComponentTemplateError::StorageMapHasDuplicateKeys(Box::new(err)))
    }

    pub fn key_schema(&self) -> &WordSchema {
        &self.key_schema
    }

    pub fn value_schema(&self) -> &WordSchema {
        &self.value_schema
    }

    pub fn default_values(&self) -> Option<BTreeMap<Word, Word>> {
        self.default_values.clone()
    }

    pub(super) fn write_into_with_optional_defaults<W: ByteWriter>(
        &self,
        target: &mut W,
        include_defaults: bool,
    ) {
        target.write(&self.description);
        let default_values = if include_defaults {
            self.default_values.clone()
        } else {
            None
        };
        target.write(&default_values);
        self.key_schema.write_into_with_optional_defaults(target, include_defaults);
        self.value_schema.write_into_with_optional_defaults(target, include_defaults);
    }

    pub(super) fn validate(&self) -> Result<(), AccountComponentTemplateError> {
        if let Some(description) = self.description.as_deref() {
            validate_description_ascii(description)?;
        }
        self.key_schema.validate()?;
        self.value_schema.validate()?;
        Ok(())
    }
}

impl Serializable for MapSlotSchema {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.write_into_with_optional_defaults(target, true);
    }
}

impl Deserializable for MapSlotSchema {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let description = Option::<String>::read_from(source)?;
        let default_values = Option::<BTreeMap<Word, Word>>::read_from(source)?;
        let key_schema = WordSchema::read_from(source)?;
        let value_schema = WordSchema::read_from(source)?;
        Ok(MapSlotSchema::new(description, default_values, key_schema, value_schema))
    }
}

// HELPER FUNCTIONS
// ================================================================================================

pub(in crate::account::component::storage) fn parse_storage_value_with_schema(
    schema: &WordSchema,
    raw_value: &WordValue,
    slot_prefix: &StorageValueName,
) -> Result<Word, AccountComponentTemplateError> {
    let word = match (schema, raw_value) {
        (_, WordValue::FullyTyped(word)) => *word,
        (WordSchema::Simple { r#type, .. }, raw_value) => {
            parse_simple_word_value(r#type, raw_value, slot_prefix)?
        },
        (WordSchema::Composite { value }, WordValue::Elements(elements)) => {
            parse_composite_elements(value, elements, slot_prefix)?
        },
        (WordSchema::Composite { .. }, WordValue::Atomic(value)) => SCHEMA_TYPE_REGISTRY
            .try_parse_word(&SchemaTypeId::native_word(), value)
            .map_err(|err| {
                AccountComponentTemplateError::InvalidInitStorageValue(
                    slot_prefix.clone(),
                    format!("failed to parse value as `word`: {err}"),
                )
            })?,
    };

    schema.validate_word_value(slot_prefix, "value", word)?;
    Ok(word)
}

fn parse_simple_word_value(
    schema_type: &SchemaTypeId,
    raw_value: &WordValue,
    slot_prefix: &StorageValueName,
) -> Result<Word, AccountComponentTemplateError> {
    match raw_value {
        WordValue::Atomic(value) => {
            SCHEMA_TYPE_REGISTRY.try_parse_word(schema_type, value).map_err(|err| {
                AccountComponentTemplateError::InvalidInitStorageValue(
                    slot_prefix.clone(),
                    format!("failed to parse value as `{}`: {err}", schema_type),
                )
            })
        },
        WordValue::Elements(elements) => {
            let felts: Vec<Felt> = elements
                .iter()
                .map(|element| {
                    SCHEMA_TYPE_REGISTRY.try_parse_felt(&SchemaTypeId::native_felt(), element)
                })
                .collect::<Result<_, _>>()
                .map_err(|err| {
                    AccountComponentTemplateError::InvalidInitStorageValue(
                        slot_prefix.clone(),
                        format!("failed to parse value element as `felt`: {err}"),
                    )
                })?;
            let felts: [Felt; 4] = felts.try_into().expect("length is 4");
            Ok(Word::from(felts))
        },
        WordValue::FullyTyped(word) => Ok(*word),
    }
}

fn parse_composite_elements(
    schema: &[FeltSchema; 4],
    elements: &[String; 4],
    slot_prefix: &StorageValueName,
) -> Result<Word, AccountComponentTemplateError> {
    let mut felts = [Felt::ZERO; 4];
    for (index, felt_schema) in schema.iter().enumerate() {
        let felt_type = felt_schema.felt_type();
        felts[index] =
            SCHEMA_TYPE_REGISTRY
                .try_parse_felt(&felt_type, &elements[index])
                .map_err(|err| {
                    AccountComponentTemplateError::InvalidInitStorageValue(
                        slot_prefix.clone(),
                        format!("failed to parse value[{index}] as `{felt_type}`: {err}"),
                    )
                })?;
    }
    Ok(Word::from(felts))
}

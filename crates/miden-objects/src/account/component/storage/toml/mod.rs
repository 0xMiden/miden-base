use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use miden_core::{Felt, Word};
use semver::Version;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::super::{
    AccountStorageSchema,
    FeltSchema,
    MapSlotSchema,
    StorageSlotSchema,
    StorageValueName,
    ValueSlotSchema,
    WordSchema,
    WordValue,
};
use super::type_registry::SCHEMA_TYPE_REGISTRY;
use crate::account::component::{AccountComponentMetadata, SchemaTypeIdentifier};
use crate::account::{AccountType, StorageSlotName};
use crate::errors::AccountComponentTemplateError;

mod init_storage_data;
mod serde_impls;

#[cfg(test)]
mod tests;

// ACCOUNT COMPONENT METADATA TOML FROM/TO
// ================================================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawAccountComponentMetadata {
    name: String,
    description: String,
    version: Version,
    supported_types: BTreeSet<AccountType>,
    #[serde(rename = "storage")]
    #[serde(default)]
    storage: RawStorageSchema,
}

impl AccountComponentMetadata {
    /// Deserializes `toml_string` and validates the resulting [AccountComponentMetadata]
    ///
    /// # Errors
    ///
    /// - If deserialization fails
    /// - If the schema specifies storage slots with duplicates.
    /// - If the schema contains invalid slot definitions.
    pub fn from_toml(toml_string: &str) -> Result<Self, AccountComponentTemplateError> {
        let raw: RawAccountComponentMetadata = toml::from_str(toml_string)
            .map_err(AccountComponentTemplateError::TomlDeserializationError)?;

        let RawStorageSchema { slot } = raw.storage;
        let mut fields = Vec::with_capacity(slot.len());

        for slot in slot {
            fields.push(slot.into_slot_schema()?);
        }

        let storage_schema = AccountStorageSchema::new(fields)?;
        Self::new(raw.name, raw.description, raw.version, raw.supported_types, storage_schema)
    }

    /// Serializes the account component metadata into a TOML string.
    pub fn to_toml(&self) -> Result<String, AccountComponentTemplateError> {
        let toml =
            toml::to_string(self).map_err(AccountComponentTemplateError::TomlSerializationError)?;
        Ok(toml)
    }
}

// ACCOUNT STORAGE SCHEMA SERIALIZATION
// ================================================================================================

/// Raw TOML storage schema, using dotted array headers:
///
/// - `[[storage.slot]]` for both value and map slots.
///
/// Slot kind is inferred by the shape of the `type` field:
/// - `type = "..."` or `type = [ ... ]` => value slot
/// - `type = { ... }` => map slot
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RawStorageSchema {
    #[serde(default)]
    slot: Vec<RawStorageSlotSchema>,
}

/// Storage slot type descriptor.
///
/// This field accepts either:
/// - a string (e.g. `"word"`, `"u16"`, `"miden::standards::auth::rpo_falcon512::pub_key"`) for
///   singular word slots,
/// - an array of 4 [`FeltSchema`] descriptors for composite word slots, or
/// - a table `{ key = ..., value = ... }` for map slots.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum RawSlotType {
    Word(RawWordType),
    Map(RawMapType),
}

/// A word type descriptor.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum RawWordType {
    Identifier(SchemaTypeIdentifier),
    WordElements(Vec<FeltSchema>),
}

/// A map type descriptor.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RawMapType {
    key: RawWordType,
    value: RawWordType,
}

impl WordValue {
    fn try_parse_as_typed_word(
        &self,
        schema_type: &SchemaTypeIdentifier,
        slot_prefix: &StorageValueName,
        label: &str,
    ) -> Result<Word, AccountComponentTemplateError> {
        let word = match self {
            WordValue::Scalar(value) => SCHEMA_TYPE_REGISTRY
                .try_parse_word(schema_type, value)
                .map_err(AccountComponentTemplateError::StorageValueParsingError)?,
            WordValue::Elements(elements) => {
                let felts = elements
                    .iter()
                    .map(|element| {
                        SCHEMA_TYPE_REGISTRY
                            .try_parse_felt(&SchemaTypeIdentifier::native_felt(), element)
                    })
                    .collect::<Result<Vec<Felt>, _>>()
                    .map_err(AccountComponentTemplateError::StorageValueParsingError)?;
                let felts: [Felt; 4] = felts.try_into().expect("length is 4");
                Word::from(felts)
            },
        };

        WordSchema::new_singular(schema_type.clone()).validate_word_value(
            slot_prefix,
            label,
            word,
        )?;
        Ok(word)
    }

    fn from_word(schema_type: &SchemaTypeIdentifier, word: Word) -> Self {
        WordValue::Scalar(SCHEMA_TYPE_REGISTRY.display_word(schema_type, word))
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RawStorageSlotSchema {
    /// The name of the storage slot, in `StorageSlotName` format (e.g.
    /// `my_project::module::slot`).
    name: String,
    #[serde(default)]
    description: Option<String>,
    /// Slot type descriptor.
    ///
    /// - If `type = { ... }`, this is a map slot.
    /// - If `type = [ ... ]`, this is a composite word slot whose schema is described by 4
    ///   [`FeltSchema`] descriptors.
    /// - Otherwise, if `type = "..."`, this is a singular word slot whose value is supplied at
    ///   instantiation time unless `default-value` is set (or the type is `void`).
    #[serde(rename = "type")]
    r#type: RawSlotType,
    /// The (overridable) default value for a singular word slot.
    #[serde(default)]
    default_value: Option<WordValue>,
    /// Default map entries.
    ///
    /// These entries must be fully-specified values. If the map should be populated at
    /// instantiation time, omit `default-values` and provide entries via init storage data.
    #[serde(default)]
    default_values: Option<Vec<RawMapEntrySchema>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RawMapEntrySchema {
    key: WordValue,
    value: WordValue,
}

impl RawStorageSlotSchema {
    fn from_slot(slot_name: &StorageSlotName, schema: &StorageSlotSchema) -> Self {
        match schema {
            StorageSlotSchema::Value(schema) => Self::from_value_slot(slot_name, schema),
            StorageSlotSchema::Map(schema) => Self::from_map_slot(slot_name, schema),
        }
    }

    fn from_value_slot(slot_name: &StorageSlotName, schema: &ValueSlotSchema) -> Self {
        let word = schema.word();
        let (r#type, default_value) = match word {
            WordSchema::Singular { r#type, default_value } => (
                RawSlotType::Word(RawWordType::Identifier(r#type.clone())),
                default_value.map(|word| WordValue::from_word(r#type, word)),
            ),
            WordSchema::Composite { value } => {
                (RawSlotType::Word(RawWordType::WordElements(value.to_vec())), None)
            },
        };

        Self {
            name: slot_name.as_str().to_string(),
            description: schema.description().cloned(),
            r#type,
            default_value,
            default_values: None,
        }
    }

    fn from_map_slot(slot_name: &StorageSlotName, schema: &MapSlotSchema) -> Self {
        let default_values = schema.default_values().map(|default_values| {
            default_values
                .into_iter()
                .map(|(key, value)| RawMapEntrySchema {
                    key: WordValue::from_word(&schema.key_schema().word_type(), key),
                    value: WordValue::from_word(&schema.value_schema().word_type(), value),
                })
                .collect()
        });

        let key_type = match schema.key_schema() {
            WordSchema::Singular { r#type, .. } => RawWordType::Identifier(r#type.clone()),
            WordSchema::Composite { value } => RawWordType::WordElements(value.to_vec()),
        };

        let value_type = match schema.value_schema() {
            WordSchema::Singular { r#type, .. } => RawWordType::Identifier(r#type.clone()),
            WordSchema::Composite { value } => RawWordType::WordElements(value.to_vec()),
        };

        Self {
            name: slot_name.as_str().to_string(),
            description: schema.description().cloned(),
            r#type: RawSlotType::Map(RawMapType { key: key_type, value: value_type }),
            default_value: None,
            default_values,
        }
    }

    fn into_slot_schema(
        self,
    ) -> Result<(StorageSlotName, StorageSlotSchema), AccountComponentTemplateError> {
        let RawStorageSlotSchema {
            name,
            description,
            r#type,
            default_value,
            default_values,
        } = self;

        let slot_name_raw = name;
        let slot_name = StorageSlotName::new(slot_name_raw.clone()).map_err(|err| {
            AccountComponentTemplateError::InvalidSchema(format!(
                "invalid storage slot name `{slot_name_raw}`: {err}"
            ))
        })?;

        let description =
            description.and_then(|d| if d.trim().is_empty() { None } else { Some(d) });

        if default_value.is_some() && default_values.is_some() {
            return Err(AccountComponentTemplateError::InvalidSchema(
                "storage slot schema cannot define both `default-value` and `default-values`"
                    .into(),
            ));
        }

        let slot_prefix = StorageValueName::from_slot_name(&slot_name);

        match (r#type, default_value, default_values) {
            // Map slot: inferred via `type = { ... }`.
            (RawSlotType::Map(map_type), None, default_values) => {
                let RawMapType { key: key_type, value: value_type } = map_type;

                let key_schema = match key_type {
                    RawWordType::Identifier(r#type) => WordSchema::new_singular(r#type),
                    RawWordType::WordElements(elements) => {
                        if elements.len() != 4 {
                            return Err(AccountComponentTemplateError::InvalidSchema(format!(
                                "`type.key` must be an array of 4 elements, got {}",
                                elements.len()
                            )));
                        }
                        let elements: [FeltSchema; 4] = elements.try_into().expect("length is 4");
                        WordSchema::new_value(elements)
                    },
                };

                let value_schema = match value_type {
                    RawWordType::Identifier(r#type) => WordSchema::new_singular(r#type),
                    RawWordType::WordElements(elements) => {
                        if elements.len() != 4 {
                            return Err(AccountComponentTemplateError::InvalidSchema(format!(
                                "`type.value` must be an array of 4 elements, got {}",
                                elements.len()
                            )));
                        }
                        let elements: [FeltSchema; 4] = elements.try_into().expect("length is 4");
                        WordSchema::new_value(elements)
                    },
                };

                let default_values = default_values
                    .map(|entries| {
                        let mut map = BTreeMap::new();
                        for (index, entry) in entries.into_iter().enumerate() {
                            let key = super::schema::parse_word_value_against_schema(
                                &key_schema,
                                &entry.key,
                                &slot_prefix,
                                format!("default-values[{index}].key").as_str(),
                            )
                            .map_err(|err| {
                                AccountComponentTemplateError::InvalidSchema(format!(
                                    "invalid map `default-values[{index}].key`: {err}"
                                ))
                            })?;
                            let value = super::schema::parse_word_value_against_schema(
                                &value_schema,
                                &entry.value,
                                &slot_prefix,
                                format!("default-values[{index}].value").as_str(),
                            )
                            .map_err(|err| {
                                AccountComponentTemplateError::InvalidSchema(format!(
                                    "invalid map `default-values[{index}].value`: {err}"
                                ))
                            })?;

                            if map.insert(key, value).is_some() {
                                return Err(AccountComponentTemplateError::InvalidSchema(format!(
                                    "map storage slot `default-values[{index}]` contains a duplicate key"
                                )));
                            }
                        }
                        Ok::<_, AccountComponentTemplateError>(map)
                    })
                    .transpose()?;

                Ok((
                    slot_name,
                    StorageSlotSchema::Map(MapSlotSchema::new(
                        description,
                        default_values,
                        key_schema,
                        value_schema,
                    )),
                ))
            },

            (RawSlotType::Map(_), Some(_), _) => Err(AccountComponentTemplateError::InvalidSchema(
                "map slots cannot define `default-value`".into(),
            )),

            // Value slot: composite type.
            (RawSlotType::Word(RawWordType::WordElements(elements)), None, None) => {
                if elements.len() != 4 {
                    return Err(AccountComponentTemplateError::InvalidSchema(format!(
                        "word slot `type` must be an array of 4 elements, got {}",
                        elements.len()
                    )));
                }
                let elements: [FeltSchema; 4] = elements.try_into().expect("length is 4");
                Ok((
                    slot_name,
                    StorageSlotSchema::Value(ValueSlotSchema::new(
                        description.clone(),
                        WordSchema::new_value(elements),
                    )),
                ))
            },

            (RawSlotType::Word(RawWordType::WordElements(_)), Some(_), _) => {
                Err(AccountComponentTemplateError::InvalidSchema(
                    "composite word slots cannot define `default-value`".into(),
                ))
            },

            // Value slot: singular type + optional default-value.
            (RawSlotType::Word(RawWordType::Identifier(r#type)), default_value, None) => {
                if r#type.as_str() == "map" {
                    return Err(AccountComponentTemplateError::InvalidSchema(
                        "value slots cannot use `type = \"map\"`; use `type = { key = <key-type>, value = <value-type>}` instead"
                            .into(),
                    ));
                }

                let word = default_value
                    .map(|default_value| {
                        default_value.try_parse_as_typed_word(
                            &r#type,
                            &slot_prefix,
                            "default value",
                        )
                    })
                    .transpose()?;

                let word_schema = match word {
                    Some(word) => WordSchema::new_singular_with_default(r#type, word),
                    None => WordSchema::new_singular(r#type),
                };

                Ok((
                    slot_name,
                    StorageSlotSchema::Value(ValueSlotSchema::new(
                        description.clone(),
                        word_schema,
                    )),
                ))
            },

            // If `default-values` is present but this is not a map slot.
            (RawSlotType::Word(_), _, Some(_)) => {
                Err(AccountComponentTemplateError::InvalidSchema(
                    "`default-values` can be specified only for map slots (use `type = { ... }`)"
                        .into(),
                ))
            },
        }
    }

    fn try_into_slot_schema<E>(self) -> Result<(StorageSlotName, StorageSlotSchema), E>
    where
        E: serde::de::Error,
    {
        self.into_slot_schema().map_err(|err| E::custom(err.to_string()))
    }
}

impl Serialize for AccountStorageSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let slots = self
            .fields()
            .iter()
            .map(|(slot_name, schema)| RawStorageSlotSchema::from_slot(slot_name, schema))
            .collect();

        RawStorageSchema { slot: slots }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AccountStorageSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawStorageSchema::deserialize(deserializer)?;
        let mut fields = Vec::with_capacity(raw.slot.len());

        for slot in raw.slot {
            let (slot_name, schema) = slot.try_into_slot_schema::<D::Error>()?;
            fields.push((slot_name, schema));
        }

        AccountStorageSchema::new(fields).map_err(D::Error::custom)
    }
}

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

        let RawStorageSchema { value, map } = raw.storage;
        let mut fields = Vec::with_capacity(value.len() + map.len());

        for slot in value {
            fields.push(slot.into_slot_schema()?);
        }
        for slot in map {
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
/// - `[[storage.value]]` for word/value slots
/// - `[[storage.map]]` for map slots
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RawStorageSchema {
    #[serde(default)]
    value: Vec<RawValueSlotSchema>,
    #[serde(default)]
    map: Vec<RawMapSlotSchema>,
}

/// Storage slot type descriptor.
///
/// This field accepts either:
/// - a string (e.g. `"word"`, `"u16"`, `"auth::rpo_falcon512::pub_key"`), or
/// - an array of 4 [`FeltSchema`] descriptors for composed word slots.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum RawSlotType {
    Identifier(SchemaTypeIdentifier),
    WordElements(Vec<FeltSchema>),
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
struct RawValueSlotSchema {
    /// The name of the storage slot, in `StorageSlotName` format (e.g.
    /// `my_project::module::slot`).
    name: String,
    #[serde(default)]
    description: Option<String>,
    /// Slot type.
    ///
    /// - If `type = "map"`, this is a map slot.
    /// - If `type = [ ... ]`, this is a composed word slot whose schema is described by 4
    ///   [`FeltSchema`] descriptors.
    /// - Otherwise, if `type` is set, this is a singular word slot whose value is supplied at
    ///   instantiation time unless `default-value` is set (or the type is `void`).
    #[serde(rename = "type")]
    #[serde(default)]
    r#type: Option<RawSlotType>,
    /// The (overridable) default value for a singular word slot.
    #[serde(default)]
    default_value: Option<WordValue>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
struct RawMapSlotSchema {
    /// The name of the storage map slot, in `StorageSlotName` format (e.g.
    /// `my_project::module::slot`).
    name: String,
    #[serde(default)]
    description: Option<String>,
    /// Default map entries.
    ///
    /// These entries must be fully-specified values. If the map should be populated at
    /// instantiation time, omit `default-values` and provide entries via init storage data.
    #[serde(default)]
    default_values: Option<Vec<RawMapEntrySchema>>,
    /// Optional key type/schema for map slots.
    ///
    /// When provided, this schema describes the shape/type of map keys. This field is optional
    /// and defaults to `"word"`.
    #[serde(default)]
    key_type: Option<RawSlotType>,
    /// Optional value type/schema for map slots.
    ///
    /// When provided, this schema describes the shape/type of map values. This field is optional
    /// and defaults to `"word"`.
    #[serde(default)]
    value_type: Option<RawSlotType>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RawMapEntrySchema {
    key: WordValue,
    value: WordValue,
}

impl RawValueSlotSchema {
    fn from_slot(slot_name: &StorageSlotName, schema: &ValueSlotSchema) -> Self {
        let word = schema.word();
        let (r#type, default_value) = match word {
            WordSchema::Singular { r#type, default_value } => (
                Some(RawSlotType::Identifier(r#type.clone())),
                default_value.map(|word| WordValue::from_word(r#type, word)),
            ),
            WordSchema::Composite { value } => {
                (Some(RawSlotType::WordElements(value.to_vec())), None)
            },
        };

        Self {
            name: slot_name.as_str().to_string(),
            description: schema.description().cloned(),
            r#type,
            default_value,
        }
    }

    fn into_slot_schema(
        self,
    ) -> Result<(StorageSlotName, StorageSlotSchema), AccountComponentTemplateError> {
        let RawValueSlotSchema { name, description, r#type, default_value } = self;

        let slot_name_raw = name;
        let slot_name = StorageSlotName::new(slot_name_raw.clone()).map_err(|err| {
            AccountComponentTemplateError::InvalidSchema(format!(
                "invalid storage slot name `{slot_name_raw}`: {err}"
            ))
        })?;

        let description =
            description.and_then(|d| if d.trim().is_empty() { None } else { Some(d) });

        let slot_prefix = StorageValueName::from_slot_name(&slot_name);

        match (r#type, default_value) {
            // Word slot with composed schema defined directly in `type = [ ... ]`.
            (Some(RawSlotType::WordElements(elements)), None) => {
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

            (Some(RawSlotType::WordElements(_)), Some(_)) => {
                Err(AccountComponentTemplateError::InvalidSchema(
                    "composed word slots cannot define `default-value`".into(),
                ))
            },

            (Some(RawSlotType::Identifier(r#type)), _)
                if r#type == SchemaTypeIdentifier::storage_map() =>
            {
                Err(AccountComponentTemplateError::InvalidSchema(
                    "value slots cannot use `type = \"map\"`; use `[[storage.map]]` instead".into(),
                ))
            },

            // Word slot with explicit type, and optional overridable default value.
            (Some(RawSlotType::Identifier(r#type)), default_value) => {
                let word = match default_value {
                    Some(default_value) => Some(default_value.try_parse_as_typed_word(
                        &r#type,
                        &slot_prefix,
                        "default value",
                    )?),
                    None => None,
                };

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

            // Word slot with implied `type = "word"` and an overridable default value.
            (None, Some(default_value)) => {
                let r#type = SchemaTypeIdentifier::native_word();
                let word = default_value.try_parse_as_typed_word(
                    &r#type,
                    &slot_prefix,
                    "default value",
                )?;
                Ok((
                    slot_name,
                    StorageSlotSchema::Value(ValueSlotSchema::new(
                        description.clone(),
                        WordSchema::new_singular_with_default(r#type, word),
                    )),
                ))
            },

            (None, None) => Err(AccountComponentTemplateError::InvalidSchema(
                "value slot schema must define either `type` or `default-value`".into(),
            )),
        }
    }

    fn try_into_slot_schema<E>(self) -> Result<(StorageSlotName, StorageSlotSchema), E>
    where
        E: serde::de::Error,
    {
        self.into_slot_schema().map_err(|err| E::custom(err.to_string()))
    }
}

impl RawMapSlotSchema {
    fn from_slot(slot_name: &StorageSlotName, schema: &MapSlotSchema) -> Self {
        let default_values = schema.default_values().map(|default_values| {
            default_values
                .into_iter()
                .map(|(key, value)| RawMapEntrySchema {
                    key: WordValue::from_word(&schema.key_schema().word_type(), key),
                    value: WordValue::from_word(&schema.value_schema().word_type(), value),
                })
                .collect()
        });

        let default_word = WordSchema::new_singular(SchemaTypeIdentifier::native_word());

        let key_type = (schema.key_schema() != &default_word).then(|| match schema.key_schema() {
            WordSchema::Singular { r#type, .. } => RawSlotType::Identifier(r#type.clone()),
            WordSchema::Composite { value } => RawSlotType::WordElements(value.to_vec()),
        });

        let value_type =
            (schema.value_schema() != &default_word).then(|| match schema.value_schema() {
                WordSchema::Singular { r#type, .. } => RawSlotType::Identifier(r#type.clone()),
                WordSchema::Composite { value } => RawSlotType::WordElements(value.to_vec()),
            });

        Self {
            name: slot_name.as_str().to_string(),
            description: schema.description().cloned(),
            default_values,
            key_type,
            value_type,
        }
    }

    fn into_slot_schema(
        self,
    ) -> Result<(StorageSlotName, StorageSlotSchema), AccountComponentTemplateError> {
        let RawMapSlotSchema {
            name,
            description,
            default_values,
            key_type,
            value_type,
        } = self;

        let slot_name_raw = name;
        let slot_name = StorageSlotName::new(slot_name_raw.clone()).map_err(|err| {
            AccountComponentTemplateError::InvalidSchema(format!(
                "invalid storage slot name `{slot_name_raw}`: {err}"
            ))
        })?;

        let description =
            description.and_then(|d| if d.trim().is_empty() { None } else { Some(d) });

        let slot_prefix = StorageValueName::from_slot_name(&slot_name);

        let key_schema = key_type
            .map(|key_type| match key_type {
                RawSlotType::Identifier(r#type) => {
                    if r#type == SchemaTypeIdentifier::storage_map() {
                        return Err(AccountComponentTemplateError::InvalidSchema(
                            "`key-type` cannot be `map`".into(),
                        ));
                    }
                    Ok(WordSchema::new_singular(r#type))
                },
                RawSlotType::WordElements(elements) => {
                    if elements.len() != 4 {
                        return Err(AccountComponentTemplateError::InvalidSchema(format!(
                            "`key-type` must be an array of 4 elements, got {}",
                            elements.len()
                        )));
                    }
                    let elements: [FeltSchema; 4] = elements.try_into().expect("length is 4");
                    Ok(WordSchema::new_value(elements))
                },
            })
            .transpose()?;

        let value_schema = value_type
            .map(|value_type| match value_type {
                RawSlotType::Identifier(r#type) => {
                    if r#type == SchemaTypeIdentifier::storage_map() {
                        return Err(AccountComponentTemplateError::InvalidSchema(
                            "`value-type` cannot be `map`".into(),
                        ));
                    }
                    Ok(WordSchema::new_singular(r#type))
                },
                RawSlotType::WordElements(elements) => {
                    if elements.len() != 4 {
                        return Err(AccountComponentTemplateError::InvalidSchema(format!(
                            "`value-type` must be an array of 4 elements, got {}",
                            elements.len()
                        )));
                    }
                    let elements: [FeltSchema; 4] = elements.try_into().expect("length is 4");
                    Ok(WordSchema::new_value(elements))
                },
            })
            .transpose()?;

        let default_word_schema = WordSchema::new_singular(SchemaTypeIdentifier::native_word());
        let key_schema_resolved = key_schema.clone().unwrap_or_else(|| default_word_schema.clone());
        let value_schema_resolved = value_schema.clone().unwrap_or(default_word_schema);

        let default_values = default_values
            .map(|entries| {
                let mut map = BTreeMap::new();
                for (index, entry) in entries.into_iter().enumerate() {
                    let key = super::schema::parse_word_value_against_schema(
                        &key_schema_resolved,
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
                        &value_schema_resolved,
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
        let mut values = Vec::new();
        let mut maps = Vec::new();

        for (slot_name, schema) in self.fields().iter() {
            match schema {
                StorageSlotSchema::Value(slot) => {
                    values.push(RawValueSlotSchema::from_slot(slot_name, slot))
                },
                StorageSlotSchema::Map(slot) => {
                    maps.push(RawMapSlotSchema::from_slot(slot_name, slot))
                },
            }
        }

        RawStorageSchema { value: values, map: maps }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AccountStorageSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawStorageSchema::deserialize(deserializer)?;
        let mut fields = Vec::with_capacity(raw.value.len() + raw.map.len());

        for value in raw.value {
            let (slot_name, schema) = value.try_into_slot_schema::<D::Error>()?;
            fields.push((slot_name, schema));
        }
        for map in raw.map {
            let (slot_name, schema) = map.try_into_slot_schema::<D::Error>()?;
            fields.push((slot_name, schema));
        }

        AccountStorageSchema::new(fields).map_err(D::Error::custom)
    }
}

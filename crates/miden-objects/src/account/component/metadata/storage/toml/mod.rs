use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use miden_core::{Felt, FieldElement, Word};
use semver::Version;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, Error, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
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
    storage: Vec<RawStorageSlotSchema>,
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

        let mut fields = Vec::with_capacity(raw.storage.len());
        for slot in raw.storage {
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

/// Storage slot type descriptor.
///
/// This field accepts either:
/// - a string (e.g. `"word"`, `"map"`, `"u16"`, `"auth::rpo_falcon512::pub_key"`), or
/// - an array of 4 [`FeltSchema`] descriptors for composed word slots.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum RawSlotType {
    Identifier(SchemaTypeIdentifier),
    WordElements(Vec<FeltSchema>),
}

impl WordValue {
    fn try_parse_as_word(&self) -> Result<Word, AccountComponentTemplateError> {
        match self {
            WordValue::Scalar(value) => {
                // For convenience, allow scalars to be specified as either:
                // - a hex word literal (requires `0x` prefix), or
                // - a felt literal (decimal or hex), embedded into a word as [0, 0, 0, <felt>].
                //
                // This keeps init TOML compact for felt-typed word schemas (e.g. `u16`), while
                // still requiring explicit `0x` prefix for full-word hex literals.
                if value.starts_with("0x") || value.starts_with("0X") {
                    SCHEMA_TYPE_REGISTRY
                        .try_parse_word(&SchemaTypeIdentifier::native_word(), value)
                        .map_err(AccountComponentTemplateError::StorageValueParsingError)
                } else {
                    SCHEMA_TYPE_REGISTRY
                        .try_parse_felt(&SchemaTypeIdentifier::native_felt(), value)
                        .map(|felt| Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, felt]))
                        .map_err(AccountComponentTemplateError::StorageValueParsingError)
                }
            },
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
                Ok(Word::from(felts))
            },
        }
    }

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
            WordValue::Elements(_) => self.try_parse_as_word()?,
        };

        WordSchema::new_singular(schema_type.clone()).validate_word_value(
            slot_prefix,
            label,
            word,
        )?;
        Ok(word)
    }

    fn from_word(schema_type: &SchemaTypeIdentifier, word: Word) -> Self {
        if SCHEMA_TYPE_REGISTRY.contains_felt_type(schema_type)
            && word[0] == Felt::ZERO
            && word[1] == Felt::ZERO
            && word[2] == Felt::ZERO
        {
            return WordValue::Scalar(render_typed_felt(schema_type, word[3]));
        }

        WordValue::Scalar(word.to_string())
    }
}

impl Serialize for WordValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            WordValue::Scalar(value) => serializer.serialize_str(value),
            WordValue::Elements(elements) => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                for element in elements.iter() {
                    seq.serialize_element(element)?;
                }
                seq.end()
            },
        }
    }
}

impl<'de> Deserialize<'de> for WordValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WordValueVisitor;

        impl<'de> Visitor<'de> for WordValueVisitor {
            type Value = WordValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a scalar word value or an array of 4 scalar elements")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(WordValue::Scalar(value.to_string()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(WordValue::Scalar(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(WordValue::Scalar(value.to_string()))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: Error,
            {
                Ok(WordValue::Scalar(value.to_string()))
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let elements: Vec<toml::Value> =
                    Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;
                if elements.len() != 4 {
                    return Err(de::Error::invalid_length(
                        elements.len(),
                        &"expected an array of 4 elements",
                    ));
                }

                let elements: Vec<String> = elements
                    .into_iter()
                    .map(|value| match value {
                        toml::Value::String(s) => Ok(s),
                        toml::Value::Integer(i) => Ok(i.to_string()),
                        toml::Value::Float(f) => Ok(f.to_string()),
                        other => Err(de::Error::custom(format!(
                            "expected a scalar value in word element array, got {other}"
                        ))),
                    })
                    .collect::<Result<_, _>>()?;

                Ok(WordValue::Elements(elements.try_into().expect("length was checked")))
            }
        }

        deserializer.deserialize_any(WordValueVisitor)
    }
}

fn render_typed_felt(schema_type: &SchemaTypeIdentifier, felt: Felt) -> String {
    match schema_type.as_str() {
        "void" => "0".into(),
        "u8" | "u16" | "u32" => felt.as_int().to_string(),
        "token_symbol" => crate::asset::TokenSymbol::try_from(felt)
            .and_then(|token| token.to_string())
            .unwrap_or_else(|_| format!("0x{:x}", felt.as_int())),
        _ => format!("0x{:x}", felt.as_int()),
    }
}

/// A schema/type descriptor for storage map keys/values.
///
/// This is similar to [`WordSchema`], but a string is interpreted as a *type identifier* (e.g.
/// `"word"`, `"u16"`) rather than as a literal word value.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RawMapWordType(WordSchema);

impl From<RawMapWordType> for WordSchema {
    fn from(value: RawMapWordType) -> Self {
        value.0
    }
}

impl From<WordSchema> for RawMapWordType {
    fn from(value: WordSchema) -> Self {
        RawMapWordType(value)
    }
}

impl Serialize for RawMapWordType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.0 {
            WordSchema::Singular { r#type, .. } => r#type.serialize(serializer),
            WordSchema::Composed { value } => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for RawMapWordType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MapWordTypeVisitor;

        impl<'de> Visitor<'de> for MapWordTypeVisitor {
            type Value = RawMapWordType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(
                    "a string type identifier, a 4-element array, or a map with `type`/`value`",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let id = SchemaTypeIdentifier::new(value.to_string())
                    .map_err(|err| E::custom(err.to_string()))?;
                Ok(RawMapWordType(WordSchema::new_singular(id)))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: Error,
            {
                self.visit_str(&value)
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let elements: Vec<FeltSchema> =
                    Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;
                if elements.len() != 4 {
                    return Err(de::Error::invalid_length(
                        elements.len(),
                        &"expected an array of 4 elements",
                    ));
                }
                let value: [FeltSchema; 4] = elements.try_into().expect("length was checked");
                Ok(RawMapWordType(WordSchema::new_value(value)))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                #[derive(Deserialize, Debug)]
                struct WordSchemaHelper {
                    value: Option<[FeltSchema; 4]>,
                    #[serde(rename = "type")]
                    r#type: Option<SchemaTypeIdentifier>,
                }

                let helper = WordSchemaHelper::deserialize(MapAccessDeserializer::new(map))?;
                if let Some(value) = helper.value {
                    Ok(RawMapWordType(WordSchema::new_value(value)))
                } else {
                    let r#type = helper.r#type.unwrap_or_else(SchemaTypeIdentifier::native_word);
                    Ok(RawMapWordType(WordSchema::new_singular(r#type)))
                }
            }
        }

        deserializer.deserialize_any(MapWordTypeVisitor)
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
    /// The (overrideable) default value for a singular word slot.
    #[serde(default)]
    default_value: Option<WordValue>,
    /// Default map entries.
    ///
    /// These entries must be fully-specified word values. If the map should be populated at
    /// instantiation time, omit `default-values` and use `type = "map"` to declare an
    /// init-populated map slot.
    #[serde(default)]
    default_values: Option<Vec<RawMapEntrySchema>>,
    /// Optional key type/schema for map slots.
    ///
    /// When provided, this schema describes the shape/type of map keys. This field is optional
    /// and defaults to `"word"`.
    #[serde(default)]
    key_type: Option<RawMapWordType>,
    /// Optional value type/schema for map slots.
    ///
    /// When provided, this schema describes the shape/type of map values. This field is optional
    /// and defaults to `"word"`.
    #[serde(default)]
    value_type: Option<RawMapWordType>,
    /// Unrecognized fields.
    #[serde(flatten)]
    extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RawMapEntrySchema {
    key: WordValue,
    value: WordValue,
}

impl RawStorageSlotSchema {
    fn from_slot(slot_name: &StorageSlotName, schema: &StorageSlotSchema) -> Self {
        match schema {
            StorageSlotSchema::Value(slot) => {
                let word = slot.word();
                let (r#type, default_value) = match word {
                    WordSchema::Singular { r#type, default_value } => (
                        Some(RawSlotType::Identifier(r#type.clone())),
                        default_value.map(|word| WordValue::from_word(r#type, word)),
                    ),
                    WordSchema::Composed { value } => {
                        (Some(RawSlotType::WordElements(value.to_vec())), None)
                    },
                };

                Self {
                    name: slot_name.as_str().to_string(),
                    description: slot.description().cloned(),
                    r#type,
                    default_value,
                    default_values: None,
                    key_type: None,
                    value_type: None,
                    extra: BTreeMap::new(),
                }
            },
            StorageSlotSchema::Map(slot) => {
                let r#type = Some(RawSlotType::Identifier(SchemaTypeIdentifier::storage_map()));
                let default_values = slot.default_values().map(|default_values| {
                    default_values
                        .into_iter()
                        .map(|(key, value)| RawMapEntrySchema {
                            key: WordValue::from_word(&SchemaTypeIdentifier::native_word(), key),
                            value: WordValue::from_word(
                                &SchemaTypeIdentifier::native_word(),
                                value,
                            ),
                        })
                        .collect()
                });

                let default_word = WordSchema::new_singular(SchemaTypeIdentifier::native_word());
                let key_type = (slot.key_schema() != &default_word)
                    .then(|| RawMapWordType(slot.key_schema().clone()));
                let value_type = (slot.value_schema() != &default_word)
                    .then(|| RawMapWordType(slot.value_schema().clone()));

                Self {
                    name: slot_name.as_str().to_string(),
                    description: slot.description().cloned(),
                    r#type,
                    default_value: None,
                    default_values,
                    key_type,
                    value_type,
                    extra: BTreeMap::new(),
                }
            },
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
            key_type,
            value_type,
            extra,
        } = self;

        let slot_name_raw = name;
        let slot_name = StorageSlotName::new(slot_name_raw.clone()).map_err(|err| {
            AccountComponentTemplateError::InvalidSchema(format!(
                "invalid storage slot name `{slot_name_raw}`: {err}"
            ))
        })?;

        let description =
            description.and_then(|d| if d.trim().is_empty() { None } else { Some(d) });

        if !extra.is_empty() {
            let mut keys = extra.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            return Err(AccountComponentTemplateError::InvalidSchema(format!(
                "storage slot schema contains unknown field(s): {}",
                keys.join(", ")
            )));
        }

        if default_value.is_some() && default_values.is_some() {
            return Err(AccountComponentTemplateError::InvalidSchema(
                "storage slot schema cannot define both `default-value` (word slot) and `default-values` (map slot)"
                    .into(),
            ));
        }

        let map_schema_overrides = key_type.is_some() || value_type.is_some();
        let indicates_map = default_values.is_some()
            || matches!(
                r#type.as_ref(),
                Some(RawSlotType::Identifier(t)) if t == &SchemaTypeIdentifier::storage_map()
            );
        if map_schema_overrides && !indicates_map {
            return Err(AccountComponentTemplateError::InvalidSchema(
                "storage slot schema cannot define `key-type`/`value-type` unless it is a map slot"
                    .into(),
            ));
        }

        let key_schema = key_type.map(Into::into);
        let value_schema = value_type.map(Into::into);

        let slot_prefix = StorageValueName::from_slot_name(&slot_name);

        if indicates_map && default_value.is_some() {
            return Err(AccountComponentTemplateError::InvalidSchema(
                "map slot schema cannot define `default-value`".into(),
            ));
        }

        match (r#type, default_value, default_values) {
            // Map slot with default entries.
            (Some(RawSlotType::Identifier(r#type)), None, Some(entries))
                if r#type == SchemaTypeIdentifier::storage_map() =>
            {
                let mut default_values = BTreeMap::new();
                for (index, entry) in entries.into_iter().enumerate() {
                    let key = entry.key.try_parse_as_word().map_err(|err| {
                        AccountComponentTemplateError::InvalidSchema(format!(
                            "invalid map `default-values[{index}].key`: {err}"
                        ))
                    })?;
                    let value = entry.value.try_parse_as_word().map_err(|err| {
                        AccountComponentTemplateError::InvalidSchema(format!(
                            "invalid map `default-values[{index}].value`: {err}"
                        ))
                    })?;

                    if default_values.insert(key, value).is_some() {
                        return Err(AccountComponentTemplateError::InvalidSchema(format!(
                            "map storage slot `default-values[{index}]` contains a duplicate key"
                        )));
                    }
                }

                Ok((
                    slot_name,
                    StorageSlotSchema::Map(MapSlotSchema::new(
                        description.clone(),
                        Some(default_values),
                        key_schema,
                        value_schema,
                    )),
                ))
            },

            // Init-populated map slot whose contents are provided at instantiation time.
            (Some(RawSlotType::Identifier(r#type)), None, None)
                if r#type == SchemaTypeIdentifier::storage_map() =>
            {
                Ok((
                    slot_name,
                    StorageSlotSchema::Map(MapSlotSchema::new(
                        description.clone(),
                        None,
                        key_schema,
                        value_schema,
                    )),
                ))
            },

            // (Unreachable) map slot with a `default-value`.
            (Some(RawSlotType::Identifier(r#type)), Some(_), None)
                if r#type == SchemaTypeIdentifier::storage_map() =>
            {
                Err(AccountComponentTemplateError::InvalidSchema(
                    "map slot schema cannot define `default-value`".into(),
                ))
            },

            // Word slot with composed schema defined directly in `type = [ ... ]`.
            (Some(RawSlotType::WordElements(elements)), None, None) => {
                if elements.len() != 4 {
                    return Err(AccountComponentTemplateError::InvalidSchema(format!(
                        "word slot `type` must be an array of 4 elements, got {}",
                        elements.len()
                    )));
                }
                let value: [FeltSchema; 4] = elements.try_into().expect("length was checked above");
                Ok((
                    slot_name,
                    StorageSlotSchema::Value(ValueSlotSchema::new(
                        description.clone(),
                        WordSchema::new_value(value),
                    )),
                ))
            },

            (Some(RawSlotType::WordElements(_)), Some(_), None) => {
                Err(AccountComponentTemplateError::InvalidSchema(
                    "composed word slots cannot define `default-value`".into(),
                ))
            },

            // Word slot with explicit type, and optional overrideable default value.
            (Some(RawSlotType::Identifier(r#type)), default_value, None)
                if r#type != SchemaTypeIdentifier::storage_map() =>
            {
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

            // Word slot with implied `type = "word"` and an overrideable default value.
            (None, Some(default_value), None) => {
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

            (None, None, None) => Err(AccountComponentTemplateError::InvalidSchema(
                "storage slot schema must define either `type`, `default-value`, or `default-values`"
                    .into(),
            )),

            (None, _, Some(_)) => Err(AccountComponentTemplateError::InvalidSchema(
                "map storage slots must have `type = \"map\"`".into(),
            )),

            (Some(_), _, Some(_)) => Err(AccountComponentTemplateError::InvalidSchema(
                "map storage slots must have `type = \"map\"`".into(),
            )),

            _ => Err(AccountComponentTemplateError::InvalidSchema(
                "invalid storage slot schema".into(),
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

impl Serialize for AccountStorageSchema {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.fields().len()))?;
        for (slot_name, schema) in self.fields().iter() {
            seq.serialize_element(&RawStorageSlotSchema::from_slot(slot_name, schema))?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for AccountStorageSchema {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw_schemas = Vec::<RawStorageSlotSchema>::deserialize(deserializer)?;
        let mut fields = Vec::with_capacity(raw_schemas.len());

        for raw in raw_schemas {
            let (slot_name, schema) = raw.try_into_slot_schema::<D::Error>()?;
            fields.push((slot_name, schema));
        }

        AccountStorageSchema::new(fields).map_err(D::Error::custom)
    }
}

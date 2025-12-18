use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use miden_core::{Felt, Word};
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, Error, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use super::placeholder::TemplateType;
use super::{
    FeltRepresentation,
    InitStorageData,
    MapEntry,
    MapRepresentation,
    MultiWordRepresentation,
    StorageEntry,
    StorageValueNameError,
    WordRepresentation,
};
use crate::account::component::FieldIdentifier;
use crate::account::component::template::storage::placeholder::{TEMPLATE_REGISTRY, TemplateFelt};
use crate::account::{AccountComponentMetadata, StorageValueName};
use crate::errors::AccountComponentTemplateError;

// ACCOUNT COMPONENT METADATA TOML FROM/TO
// ================================================================================================

impl AccountComponentMetadata {
    /// Deserializes `toml_string` and validates the resulting [AccountComponentMetadata]
    ///
    /// # Errors
    ///
    /// - If deserialization fails
    /// - If the template specifies storage slots with duplicates.
    /// - If the template includes slot numbers that do not start at zero.
    /// - If storage slots in the template are not contiguous.
    pub fn from_toml(toml_string: &str) -> Result<Self, AccountComponentTemplateError> {
        let component: AccountComponentMetadata = toml::from_str(toml_string)
            .map_err(AccountComponentTemplateError::TomlDeserializationError)?;

        component.validate()?;
        Ok(component)
    }

    /// Serializes the account component template into a TOML string.
    pub fn to_toml(&self) -> Result<String, AccountComponentTemplateError> {
        let toml =
            toml::to_string(self).map_err(AccountComponentTemplateError::TomlSerializationError)?;
        Ok(toml)
    }
}

// WORD REPRESENTATION SERIALIZATION
// ================================================================================================

impl Serialize for WordRepresentation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            WordRepresentation::Template { identifier, r#type } => {
                let mut state = serializer.serialize_struct("WordRepresentation", 3)?;
                state.serialize_field("name", &identifier.name())?;
                state.serialize_field("description", &identifier.description())?;
                state.serialize_field("type", r#type)?;
                state.end()
            },
            WordRepresentation::Value { identifier, value } => {
                let mut state = serializer.serialize_struct("WordRepresentation", 3)?;

                state.serialize_field("name", &identifier.as_ref().map(|id| id.name()))?;
                state.serialize_field(
                    "description",
                    &identifier.as_ref().map(|id| id.description()),
                )?;
                state.serialize_field("value", value)?;
                state.end()
            },
        }
    }
}

impl<'de> Deserialize<'de> for WordRepresentation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct WordRepresentationVisitor;

        impl<'de> Visitor<'de> for WordRepresentationVisitor {
            type Value = WordRepresentation;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string or a map representing a WordRepresentation")
            }

            // A bare string is interpreted it as a Value variant.
            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                let parsed_value = Word::parse(value).map_err(|_err| {
                    E::invalid_value(
                        serde::de::Unexpected::Str(value),
                        &"a valid hexadecimal string",
                    )
                })?;
                Ok(<[Felt; _]>::from(&parsed_value).into())
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
                // Deserialize as a list of felt representations
                let elements: Vec<FeltRepresentation> =
                    Deserialize::deserialize(serde::de::value::SeqAccessDeserializer::new(seq))?;
                if elements.len() != 4 {
                    return Err(Error::invalid_length(
                        elements.len(),
                        &"expected an array of 4 elements",
                    ));
                }
                let value: [FeltRepresentation; 4] =
                    elements.try_into().expect("length was checked");
                Ok(WordRepresentation::new_value(value, None))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'de>,
            {
                #[derive(Deserialize, Debug)]
                struct WordRepresentationHelper {
                    name: Option<String>,
                    description: Option<String>,
                    // The "value" field (if present) must be an array of 4 FeltRepresentations.
                    value: Option<[FeltRepresentation; 4]>,
                    #[serde(rename = "type")]
                    r#type: Option<TemplateType>,
                }

                let helper =
                    WordRepresentationHelper::deserialize(MapAccessDeserializer::new(map))?;

                if let Some(value) = helper.value {
                    let identifier = helper
                        .name
                        .map(|n| parse_field_identifier::<M::Error>(n, helper.description.clone()))
                        .transpose()?;
                    Ok(WordRepresentation::Value { value, identifier })
                } else {
                    // Otherwise, we expect a Template variant (name is required for identification)
                    let identifier = expect_parse_field_identifier::<M::Error>(
                        helper.name,
                        helper.description,
                        "word template",
                    )?;
                    let r#type = helper.r#type.unwrap_or_else(TemplateType::native_word);
                    Ok(WordRepresentation::Template { r#type, identifier })
                }
            }
        }

        deserializer.deserialize_any(WordRepresentationVisitor)
    }
}

// FELT REPRESENTATION SERIALIZATION
// ================================================================================================

impl Serialize for FeltRepresentation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            FeltRepresentation::Value { identifier, value } => {
                let hex = value.to_string();
                if identifier.is_none() {
                    serializer.serialize_str(&hex)
                } else {
                    let mut state = serializer.serialize_struct("FeltRepresentation", 3)?;
                    if let Some(id) = identifier {
                        state.serialize_field("name", &id.name)?;
                        state.serialize_field("description", &id.description)?;
                    }
                    state.serialize_field("value", &hex)?;
                    state.end()
                }
            },
            FeltRepresentation::Template { identifier, r#type } => {
                let mut state = serializer.serialize_struct("FeltRepresentation", 3)?;
                state.serialize_field("name", &identifier.name)?;
                state.serialize_field("description", &identifier.description)?;
                state.serialize_field("type", r#type)?;
                state.end()
            },
        }
    }
}

impl<'de> Deserialize<'de> for FeltRepresentation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Felts can be deserialized as either:
        //
        // - Scalars (parsed from strings)
        // - A table object that can or cannot hardcode a value. If not present, this is a
        //   placeholder type
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Intermediate {
            Map {
                name: Option<String>,
                description: Option<String>,
                #[serde(default)]
                value: Option<String>,
                #[serde(rename = "type")]
                r#type: Option<TemplateType>,
            },
            Scalar(String),
        }

        let intermediate = Intermediate::deserialize(deserializer)?;
        match intermediate {
            Intermediate::Scalar(s) => {
                let felt = Felt::parse_felt(&s)
                    .map_err(|e| D::Error::custom(format!("failed to parse Felt: {e}")))?;
                Ok(FeltRepresentation::Value { identifier: None, value: felt })
            },
            Intermediate::Map { name, description, value, r#type } => {
                // Get the defined type, or the default if it was not specified
                let felt_type = r#type.unwrap_or_else(TemplateType::native_felt);
                if let Some(val_str) = value {
                    // Parse into felt from the input string
                    let felt =
                        TEMPLATE_REGISTRY.try_parse_felt(&felt_type, &val_str).map_err(|e| {
                            D::Error::custom(format!("failed to parse {felt_type} as Felt: {e}"))
                        })?;
                    let identifier = name
                        .map(|n| parse_field_identifier::<D::Error>(n, description.clone()))
                        .transpose()?;
                    Ok(FeltRepresentation::Value { identifier, value: felt })
                } else {
                    // No value provided, so this is a placeholder
                    let identifier = expect_parse_field_identifier::<D::Error>(
                        name,
                        description,
                        "map template",
                    )?;
                    Ok(FeltRepresentation::Template { r#type: felt_type, identifier })
                }
            },
        }
    }
}

// STORAGE VALUES
// ================================================================================================

/// Represents the type of values that can be found in a storage slot's `values` field.
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum StorageValues {
    /// List of individual words (for multi-slot entries).
    Words(Vec<[FeltRepresentation; 4]>),
    /// List of key-value entries (for map storage slots).
    MapEntries(Vec<MapEntry>),
}

// STORAGE ENTRY SERIALIZATION
// ================================================================================================

#[derive(Default, Debug, Deserialize, Serialize)]
struct RawStorageEntry {
    #[serde(flatten)]
    identifier: Option<FieldIdentifier>,
    slot: Option<u8>,
    slots: Option<Vec<u8>>,
    #[serde(rename = "type")]
    word_type: Option<TemplateType>,
    value: Option<[FeltRepresentation; 4]>,
    values: Option<StorageValues>,
}

impl From<StorageEntry> for RawStorageEntry {
    fn from(entry: StorageEntry) -> Self {
        match entry {
            StorageEntry::Value { slot, word_entry } => match word_entry {
                WordRepresentation::Value { identifier, value } => RawStorageEntry {
                    slot: Some(slot),
                    identifier,
                    value: Some(value),
                    ..Default::default()
                },
                WordRepresentation::Template { identifier, r#type } => RawStorageEntry {
                    slot: Some(slot),
                    identifier: Some(identifier),
                    word_type: Some(r#type),
                    ..Default::default()
                },
            },
            StorageEntry::Map { slot, map } => match map {
                MapRepresentation::Value { identifier, entries } => RawStorageEntry {
                    slot: Some(slot),
                    identifier: Some(FieldIdentifier {
                        name: identifier.name,
                        description: identifier.description,
                    }),
                    values: Some(StorageValues::MapEntries(entries)),
                    ..Default::default()
                },
                MapRepresentation::Template { identifier } => RawStorageEntry {
                    slot: Some(slot),
                    identifier: Some(FieldIdentifier {
                        name: identifier.name,
                        description: identifier.description,
                    }),
                    word_type: Some(TemplateType::storage_map()),
                    ..Default::default()
                },
            },
            StorageEntry::MultiSlot { slots, word_entries } => match word_entries {
                MultiWordRepresentation::Value { identifier, values } => RawStorageEntry {
                    slot: None,
                    identifier: Some(identifier),
                    slots: Some(slots.collect()),
                    values: Some(StorageValues::Words(values)),
                    ..Default::default()
                },
            },
        }
    }
}

impl Serialize for StorageEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let raw_storage_entry: RawStorageEntry = self.clone().into();
        raw_storage_entry.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StorageEntry {
    fn deserialize<D>(deserializer: D) -> Result<StorageEntry, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawStorageEntry::deserialize(deserializer)?;

        if let Some(word_entry) = raw.value {
            // If a value was provided, this is a WordRepresentation::Value entry
            let slot = raw.slot.ok_or_else(|| missing_field_for("slot", "value entry"))?;
            let identifier = raw.identifier;
            Ok(StorageEntry::Value {
                slot,
                word_entry: WordRepresentation::Value { value: word_entry, identifier },
            })
        } else if let Some(StorageValues::MapEntries(map_entries)) = raw.values {
            // If `values` field contains key/value pairs, deserialize as map
            let identifier =
                raw.identifier.ok_or_else(|| missing_field_for("identifier", "map entry"))?;
            let name = identifier.name;
            let slot = raw.slot.ok_or_else(|| missing_field_for("slot", "map entry"))?;
            if let Some(word_type) = raw.word_type.clone()
                && word_type != TemplateType::storage_map()
            {
                return Err(D::Error::custom(
                    "map storage entries with `values` must have `type = \"map\"`",
                ));
            }
            let mut map = MapRepresentation::new_value(map_entries, name);
            if let Some(desc) = identifier.description {
                map = map.with_description(desc);
            }
            Ok(StorageEntry::Map { slot, map })
        } else if let Some(word_type) = raw.word_type.clone()
            && word_type == TemplateType::storage_map()
        {
            let identifier =
                raw.identifier.ok_or_else(|| missing_field_for("identifier", "map entry"))?;
            let slot = raw.slot.ok_or_else(|| missing_field_for("slot", "map entry"))?;
            let FieldIdentifier { name, description } = identifier;

            // If values is specified (even if empty), create a value map.
            // Due to #[serde(untagged)] on StorageValues, values = [] gets deserialized
            // as StorageValues::Words(vec![]), so we need to treat it as an empty map.
            // Otherwise, create a template map.
            let mut map = if raw.values.is_some() {
                MapRepresentation::new_value(Vec::new(), name)
            } else {
                MapRepresentation::new_template(name)
            };

            if let Some(desc) = description {
                map = map.with_description(desc);
            }
            Ok(StorageEntry::Map { slot, map })
        } else if let Some(StorageValues::Words(values)) = raw.values {
            let identifier = raw
                .identifier
                .ok_or_else(|| missing_field_for("identifier", "multislot entry"))?;

            let mut slots =
                raw.slots.ok_or_else(|| missing_field_for("slots", "multislot entry"))?;

            // Sort so we can check contiguity
            slots.sort_unstable();
            for pair in slots.windows(2) {
                if pair[1] != pair[0] + 1 {
                    return Err(serde::de::Error::custom(format!(
                        "`slots` in the `{}` storage entry are not contiguous",
                        identifier.name
                    )));
                }
            }
            let start = slots[0];
            let end = slots.last().expect("checked validity") + 1;
            Ok(StorageEntry::new_multislot(identifier, start..end, values))
        } else if let Some(word_type) = raw.word_type {
            // If a type was provided instead, this is a WordRepresentation::Template entry
            let slot = raw.slot.ok_or_else(|| missing_field_for("slot", "single-slot entry"))?;
            let identifier = raw
                .identifier
                .ok_or_else(|| missing_field_for("identifier", "single-slot entry"))?;
            let word_entry = WordRepresentation::Template { r#type: word_type, identifier };
            Ok(StorageEntry::Value { slot, word_entry })
        } else {
            Err(D::Error::custom("placeholder storage entries require the `type` field"))
        }
    }
}

// INIT STORAGE DATA
// ================================================================================================

impl InitStorageData {
    /// Creates an instance of [`InitStorageData`] from a TOML string.
    ///
    /// This method parses the provided TOML and flattens nested tables into
    /// dot‑separated keys using [`StorageValueName`] as keys. All values are converted to plain
    /// strings (so that, for example, `key = 10` and `key = "10"` both yield
    /// `String::from("10")` as the value).
    ///
    /// # Errors
    ///
    /// - If duplicate keys or empty tables are found in the string
    /// - If the TOML string includes arrays
    pub fn from_toml(toml_str: &str) -> Result<Self, InitStorageDataError> {
        let value: toml::Value = toml::from_str(toml_str)?;
        let mut value_entries = BTreeMap::new();
        let mut map_entries = BTreeMap::new();
        // Start with an empty prefix (i.e. the default, which is an empty string)
        Self::flatten_parse_toml_value(
            StorageValueName::empty(),
            value,
            &mut value_entries,
            &mut map_entries,
        )?;

        Ok(InitStorageData::new(value_entries, map_entries))
    }

    /// Recursively flattens a TOML `Value` into a flat mapping.
    ///
    /// When recursing into nested tables, keys are combined using
    /// [`StorageValueName::with_suffix`]. If an encountered table is empty (and not the top-level),
    /// an error is returned. Arrays are not supported.
    fn flatten_parse_toml_value(
        prefix: StorageValueName,
        value: toml::Value,
        value_entries: &mut BTreeMap<StorageValueName, String>,
        map_entries: &mut BTreeMap<StorageValueName, Vec<(Word, Word)>>,
    ) -> Result<(), InitStorageDataError> {
        match value {
            toml::Value::Table(table) => {
                // If this is not the root and the table is empty, error
                if !prefix.as_str().is_empty() && table.is_empty() {
                    return Err(InitStorageDataError::EmptyTable(prefix.as_str().into()));
                }
                for (key, val) in table {
                    // Create a new key and combine it with the current prefix.
                    let new_key = StorageValueName::new(key.to_string())
                        .map_err(InitStorageDataError::InvalidStorageValueName)?;
                    let new_prefix = prefix.clone().with_suffix(&new_key);
                    Self::flatten_parse_toml_value(new_prefix, val, value_entries, map_entries)?;
                }
            },
            toml::Value::Array(items) if items.is_empty() => {
                if prefix.as_str().is_empty() {
                    return Err(InitStorageDataError::ArraysNotSupported);
                }
                map_entries.insert(prefix, Vec::new());
            },
            toml::Value::Array(items) => {
                if prefix.as_str().is_empty()
                    || !items.iter().all(|item| matches!(item, toml::Value::Table(_)))
                {
                    return Err(InitStorageDataError::ArraysNotSupported);
                }

                let entries = items
                    .into_iter()
                    .map(parse_map_entry_value)
                    .collect::<Result<Vec<(Word, Word)>, _>>()?;
                map_entries.insert(prefix, entries);
            },
            toml_value => {
                // Get the string value, or convert to string if it's some other type
                let value = match toml_value {
                    toml::Value::String(s) => s.clone(),
                    _ => toml_value.to_string(),
                };
                value_entries.insert(prefix, value);
            },
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum InitStorageDataError {
    #[error("failed to parse TOML")]
    InvalidToml(#[from] toml::de::Error),

    #[error("empty table encountered for key `{0}`")]
    EmptyTable(String),

    #[error("invalid input: arrays are not supported")]
    ArraysNotSupported,

    #[error("invalid storage value name")]
    InvalidStorageValueName(#[source] StorageValueNameError),

    #[error("invalid map entry: {0}")]
    InvalidMapEntry(String),
}

impl Serialize for FieldIdentifier {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("name", &self.name)?;
        map.serialize_entry("description", &self.description)?;
        map.end()
    }
}

struct FieldIdentifierVisitor;

impl<'de> Visitor<'de> for FieldIdentifierVisitor {
    type Value = FieldIdentifier;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a map with 'name' and optionally 'description'")
    }

    fn visit_map<M>(self, mut map: M) -> Result<FieldIdentifier, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut name = None;
        let mut description = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "name" => {
                    name = Some(map.next_value()?);
                },
                "description" => {
                    let d: String = map.next_value()?;
                    // Normalize empty or whitespace-only strings into None
                    description = if d.trim().is_empty() { None } else { Some(d) };
                },
                _ => {
                    // Ignore other values as FieldIdentifiers are flattened within other structs
                    let _: de::IgnoredAny = map.next_value()?;
                },
            }
        }
        let name = name.ok_or_else(|| de::Error::missing_field("name"))?;
        Ok(FieldIdentifier { name, description })
    }
}

impl<'de> Deserialize<'de> for FieldIdentifier {
    fn deserialize<D>(deserializer: D) -> Result<FieldIdentifier, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(FieldIdentifierVisitor)
    }
}

// UTILS / HELPERS
// ================================================================================================

fn missing_field_for<E: serde::de::Error>(field: &str, context: &str) -> E {
    E::custom(format!("missing '{field}' field for {context}"))
}

/// Checks than an optional (but expected) name field has been defined and is correct.
fn expect_parse_field_identifier<E: serde::de::Error>(
    n: Option<String>,
    description: Option<String>,
    context: &str,
) -> Result<FieldIdentifier, E> {
    let name = n.ok_or_else(|| missing_field_for("name", context))?;
    parse_field_identifier(name, description)
}

/// Tries to parse a string into a [FieldIdentifier].
fn parse_field_identifier<E: serde::de::Error>(
    n: String,
    description: Option<String>,
) -> Result<FieldIdentifier, E> {
    StorageValueName::new(n)
        .map_err(|err| E::custom(format!("invalid `name`: {err}")))
        .map(|storage_name| {
            if let Some(desc) = description {
                FieldIdentifier::with_description(storage_name, desc)
            } else {
                FieldIdentifier::with_name(storage_name)
            }
        })
}

/// Parses a `{ key, value }` TOML table into a `(Word, Word)` pair, rejecting templates.
fn parse_map_entry_value(item: toml::Value) -> Result<(Word, Word), InitStorageDataError> {
    // Try to deserialize the user input as a map entry
    let entry: MapEntry = MapEntry::deserialize(item)
        .map_err(|err| InitStorageDataError::InvalidMapEntry(err.to_string()))?;

    // Make sure the entry does not contain templates, only static
    if entry.key().template_requirements(StorageValueName::empty()).next().is_some()
        || entry.value().template_requirements(StorageValueName::empty()).next().is_some()
    {
        return Err(InitStorageDataError::InvalidMapEntry(
            "map entries cannot contain templates".into(),
        ));
    }

    // Interpret the user input as static words
    let key = entry
        .key()
        .try_build_word(&InitStorageData::default(), StorageValueName::empty())
        .map_err(|err| InitStorageDataError::InvalidMapEntry(err.to_string()))?;
    let value = entry
        .value()
        .try_build_word(&InitStorageData::default(), StorageValueName::empty())
        .map_err(|err| InitStorageDataError::InvalidMapEntry(err.to_string()))?;

    Ok((key, value))
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use alloc::string::ToString;
    use core::error::Error;

    use super::*;
    use crate::account::component::toml::InitStorageDataError;

    #[test]
    fn from_toml_str_with_nested_table_and_flattened() {
        let toml_table = r#"
            [token_metadata]
            max_supply = "1000000000"
            symbol = "ETH"
            decimals = "9"
        "#;

        let toml_inline = r#"
            token_metadata.max_supply = "1000000000"
            token_metadata.symbol = "ETH"
            token_metadata.decimals = "9"
        "#;

        let storage_table = InitStorageData::from_toml(toml_table).unwrap();
        let storage_inline = InitStorageData::from_toml(toml_inline).unwrap();

        assert_eq!(storage_table.placeholders(), storage_inline.placeholders());
    }

    #[test]
    fn from_toml_str_with_deeply_nested_tables() {
        let toml_str = r#"
            [a]
            b = "0xb"

            [a.c]
            d = "0xd"

            [x.y.z]
            w = 42 # NOTE: This gets parsed as string
        "#;

        let storage = InitStorageData::from_toml(toml_str).expect("Failed to parse TOML");
        let key1 = StorageValueName::new("a.b".to_string()).unwrap();
        let key2 = StorageValueName::new("a.c.d".to_string()).unwrap();
        let key3 = StorageValueName::new("x.y.z.w".to_string()).unwrap();

        assert_eq!(storage.get(&key1).unwrap(), "0xb");
        assert_eq!(storage.get(&key2).unwrap(), "0xd");
        assert_eq!(storage.get(&key3).unwrap(), "42");
    }

    #[test]
    fn test_error_on_array() {
        let toml_str = r#"
            token_metadata.v = [1, 2, 3]
        "#;

        let result = InitStorageData::from_toml(toml_str);
        assert_matches::assert_matches!(
            result.unwrap_err(),
            InitStorageDataError::ArraysNotSupported
        );
    }

    #[test]
    fn parse_map_entries_from_array() {
        let toml_str = r#"
            my_map = [
                { key = "0x0000000000000000000000000000000000000000000000000000000000000001", value = "0x0000000000000000000000000000000000000000000000000000000000000010" },
                { key = "0x0000000000000000000000000000000000000000000000000000000000000002", value = ["1", "2", "3", "4"] }
            ]
        "#;

        let storage = InitStorageData::from_toml(toml_str).expect("Failed to parse map entries");
        let map_name = StorageValueName::new("my_map").unwrap();
        let entries = storage.map_entries(&map_name).expect("map entries missing");
        assert_eq!(entries.len(), 2);

        let first_key =
            Word::try_from("0x0000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        assert_eq!(entries[0].0, first_key);

        let second_value =
            Word::from([Felt::new(1u64), Felt::new(2u64), Felt::new(3u64), Felt::new(4u64)]);
        assert_eq!(entries[1].1, second_value);
    }

    #[test]
    fn error_on_empty_subtable() {
        let toml_str = r#"
            [a]
            b = {}
        "#;

        let result = InitStorageData::from_toml(toml_str);
        assert_matches::assert_matches!(result.unwrap_err(), InitStorageDataError::EmptyTable(_));
    }

    #[test]
    fn error_on_duplicate_keys() {
        let toml_str = r#"
            token_metadata.max_supply = "1000000000"
            token_metadata.max_supply = "500000000"
        "#;

        let result = InitStorageData::from_toml(toml_str).unwrap_err();
        // TOML does not support duplicate keys
        assert_matches::assert_matches!(result, InitStorageDataError::InvalidToml(_));
        assert!(result.source().unwrap().to_string().contains("duplicate"));
    }
}

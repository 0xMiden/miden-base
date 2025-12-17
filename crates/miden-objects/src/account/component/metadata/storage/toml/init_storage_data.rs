use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Deserialize;
use thiserror::Error;

use super::super::{InitStorageData, StorageValueName, StorageValueNameError, WordValue};
use crate::account::component::toml::RawMapEntrySchema;

impl InitStorageData {
    /// Creates an instance of [`InitStorageData`] from a TOML string.
    ///
    /// This method parses the provided TOML and flattens nested tables into
    /// dot‑separated keys using [`StorageValueName`] as keys.
    ///
    /// Scalar values are stored as [`WordValue::Scalar`] strings (so that, for example,
    /// `key = 10` and `key = "10"` both yield `WordValue::Scalar("10")`).
    ///
    /// Arrays are supported for:
    /// - storage map slots: an array of inline tables of the form `{ key = <word>, value = <word>
    ///   }`,
    /// - word values: a 4-element array of scalar elements.
    ///
    /// # Errors
    ///
    /// - If duplicate keys or empty tables are found in the string
    /// - If the TOML string includes unsupported arrays
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
    /// an error is returned.
    fn flatten_parse_toml_value(
        prefix: StorageValueName,
        value: toml::Value,
        value_entries: &mut BTreeMap<StorageValueName, WordValue>,
        map_entries: &mut BTreeMap<StorageValueName, Vec<(WordValue, WordValue)>>,
    ) -> Result<(), InitStorageDataError> {
        match value {
            toml::Value::Table(table) => {
                // If this is not the root and the table is empty, error
                if !prefix.as_str().is_empty() && table.is_empty() {
                    return Err(InitStorageDataError::EmptyTable(prefix.as_str().into()));
                }
                for (key, val) in table {
                    // Create a new key and combine it with the current prefix.
                    let new_key: StorageValueName =
                        key.parse().map_err(InitStorageDataError::InvalidStorageValueName)?;
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
                if prefix.as_str().is_empty() {
                    return Err(InitStorageDataError::ArraysNotSupported);
                }

                // Arrays can be either:
                // - map entries: an array of inline tables `{ key = ..., value = ... }`
                // - a 4-element word value: an array of 4 scalar elements
                if items.iter().all(|item| matches!(item, toml::Value::Table(_))) {
                    let entries = items.into_iter().map(parse_map_entry_value).collect::<Result<
                        Vec<(WordValue, WordValue)>,
                        _,
                    >>(
                    )?;
                    map_entries.insert(prefix, entries);
                } else if items.len() == 4
                    && items.iter().all(|item| {
                        matches!(
                            item,
                            toml::Value::String(_)
                                | toml::Value::Integer(_)
                                | toml::Value::Float(_)
                        )
                    })
                {
                    let elements: Vec<String> = items
                        .into_iter()
                        .map(|value| match value {
                            toml::Value::String(s) => Ok(s),
                            toml::Value::Integer(i) => Ok(i.to_string()),
                            toml::Value::Float(f) => Ok(f.to_string()),
                            _ => Err(InitStorageDataError::ArraysNotSupported),
                        })
                        .collect::<Result<_, _>>()
                        .map_err(|_| InitStorageDataError::ArraysNotSupported)?;

                    let elements: [String; 4] =
                        elements.try_into().expect("length was checked above");
                    value_entries.insert(prefix, WordValue::Elements(elements));
                } else {
                    return Err(InitStorageDataError::ArraysNotSupported);
                }
            },
            toml_value => {
                // Get the string value, or convert to string if it's some other type
                let value = match toml_value {
                    toml::Value::String(s) => s.clone(),
                    _ => toml_value.to_string(),
                };
                value_entries.insert(prefix, WordValue::Scalar(value));
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

    #[error("invalid input: unsupported array value")]
    ArraysNotSupported,

    #[error("invalid storage value name")]
    InvalidStorageValueName(#[source] StorageValueNameError),

    #[error("invalid map entry: {0}")]
    InvalidMapEntrySchema(String),
}

/// Parses a `{ key, value }` TOML table into a `(Word, Word)` pair, rejecting typed fields.
fn parse_map_entry_value(
    item: toml::Value,
) -> Result<(WordValue, WordValue), InitStorageDataError> {
    // Try to deserialize the user input as a map entry
    let entry: RawMapEntrySchema = RawMapEntrySchema::deserialize(item)
        .map_err(|err| InitStorageDataError::InvalidMapEntrySchema(err.to_string()))?;

    Ok((entry.key, entry.value))
}

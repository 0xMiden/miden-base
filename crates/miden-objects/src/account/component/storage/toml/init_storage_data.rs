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
    /// Atomic values must be strings (e.g. `"0x1234"`, `"16"`, `"BTC"`).
    ///
    /// Arrays are supported for:
    /// - storage map slots: an array of inline tables of the form `{ key = <word>, value = <word>
    ///   }`,
    /// - word values: a 4-element array of field elements.
    ///
    /// # Errors
    ///
    /// - If the TOML string fails to parse
    /// - If duplicate keys are found after parsing
    /// - If empty tables are found in the string
    /// - If the TOML string includes unsupported arrays
    pub fn from_toml(toml_str: &str) -> Result<Self, InitStorageDataError> {
        // TOML documents are always parsed as a root table.
        let table: toml::Table = toml::from_str(toml_str)?;
        let mut value_entries = BTreeMap::new();
        let mut map_entries = BTreeMap::new();
        // Start with an empty prefix (i.e. the default, which is an empty string)
        Self::flatten_parse_toml_value(
            StorageValueName::empty(),
            toml::Value::Table(table),
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
                if value_entries.contains_key(&prefix) || map_entries.contains_key(&prefix) {
                    return Err(InitStorageDataError::DuplicateKey(prefix.as_str().into()));
                }
                map_entries.insert(prefix, Vec::new());
            },
            toml::Value::Array(items) => {
                // Arrays can be either:
                // - map entries: an array of inline tables `{ key = ..., value = ... }`
                // - a 4-element word value: an array of 4 field elements
                if items.iter().all(|item| matches!(item, toml::Value::Table(_))) {
                    let entries = items.into_iter().map(parse_map_entry_value).collect::<Result<
                        Vec<(WordValue, WordValue)>,
                        _,
                    >>(
                    )?;
                    if value_entries.contains_key(&prefix) || map_entries.contains_key(&prefix) {
                        return Err(InitStorageDataError::DuplicateKey(prefix.as_str().into()));
                    }
                    map_entries.insert(prefix, entries);
                } else if items.len() == 4
                    && items.iter().all(|item| matches!(item, toml::Value::String(_)))
                {
                    let elements: [String; 4] = items
                        .into_iter()
                        .map(|value| match value {
                            toml::Value::String(s) => Ok(s),
                            _ => Err(InitStorageDataError::ArraysNotSupported {
                                key: prefix.as_str().into(),
                                len: 4,
                            }),
                        })
                        .collect::<Result<Vec<_>, _>>()?
                        .try_into()
                        .expect("length was checked above");
                    if value_entries.contains_key(&prefix) || map_entries.contains_key(&prefix) {
                        return Err(InitStorageDataError::DuplicateKey(prefix.as_str().into()));
                    }
                    value_entries.insert(prefix, WordValue::Elements(elements));
                } else {
                    return Err(InitStorageDataError::ArraysNotSupported {
                        key: prefix.as_str().into(),
                        len: items.len(),
                    });
                }
            },
            toml_value => match toml_value {
                toml::Value::String(s) => {
                    if value_entries.contains_key(&prefix) || map_entries.contains_key(&prefix) {
                        return Err(InitStorageDataError::DuplicateKey(prefix.as_str().into()));
                    }
                    value_entries.insert(prefix, WordValue::Atomic(s));
                },
                _ => {
                    return Err(InitStorageDataError::NonStringAtomic(prefix.as_str().into()));
                },
            },
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum InitStorageDataError {
    #[error("failed to parse TOML: {0}")]
    InvalidToml(#[from] toml::de::Error),

    #[error("empty table encountered for key `{0}`")]
    EmptyTable(String),

    #[error("duplicate init key `{0}`")]
    DuplicateKey(String),

    #[error(
        "invalid input for `{key}`: unsupported array value (len {len}); expected either a map entry list (array of inline tables with `key` and `value`) or a 4-element word array of strings"
    )]
    ArraysNotSupported { key: String, len: usize },

    #[error("invalid input for `{0}`: init values must be strings")]
    NonStringAtomic(String),

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

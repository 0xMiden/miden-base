use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::StorageValueName;

/// A raw word value provided via [`InitStorageData`].
///
/// This is used for defining specific values in relation to a component's schema, where each values
/// is supplied as either an atomic string (e.g. `"0x1234"`, `"16"`, `"BTC"`) or an array of 4 field
/// elements.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "std", serde(untagged))]
pub enum WordValue {
    /// Represents a single word value, given by a single string input.
    Atomic(String),
    /// Represents a word through four string-encoded field elements.
    Elements([String; 4]),
}

impl From<String> for WordValue {
    fn from(value: String) -> Self {
        WordValue::Atomic(value)
    }
}

impl From<&str> for WordValue {
    fn from(value: &str) -> Self {
        WordValue::Atomic(String::from(value))
    }
}

/// Represents the data required to initialize storage entries when instantiating an
/// [AccountComponent](crate::account::AccountComponent) from component metadata (either provided
/// directly or extracted from a package).
///
/// An [`InitStorageData`] can be created from a TOML string when the `std` feature flag is set.
#[derive(Clone, Debug, Default)]
pub struct InitStorageData {
    /// A mapping of init value names to their raw values.
    value_entries: BTreeMap<StorageValueName, WordValue>,
    /// A mapping of storage map slot names to their raw key/value entries.
    map_entries: BTreeMap<StorageValueName, Vec<(WordValue, WordValue)>>,
}

impl InitStorageData {
    /// Creates a new instance of [InitStorageData].
    ///
    /// A [`BTreeMap`] is constructed from the passed iterator, so duplicate keys will cause
    /// overridden values.
    pub fn new(
        entries: impl IntoIterator<Item = (StorageValueName, WordValue)>,
        map_entries: impl IntoIterator<Item = (StorageValueName, Vec<(WordValue, WordValue)>)>,
    ) -> Self {
        InitStorageData {
            value_entries: entries.into_iter().collect(),
            map_entries: map_entries.into_iter().collect(),
        }
    }

    /// Returns a reference to the underlying init values map.
    pub fn values(&self) -> &BTreeMap<StorageValueName, WordValue> {
        &self.value_entries
    }

    /// Returns a reference to the stored init value, or [`Option::None`] if the key is not
    /// present.
    pub fn get(&self, key: &StorageValueName) -> Option<&WordValue> {
        self.value_entries.get(key)
    }

    /// Returns the map entries associated with the given storage map slot name, if any.
    pub fn map_entries(&self, key: &StorageValueName) -> Option<&Vec<(WordValue, WordValue)>> {
        self.map_entries.get(key)
    }
}

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::StorageValueName;
use crate::Word;

/// Represents the data required to initialize storage entries when instantiating an
/// [AccountComponent](crate::account::AccountComponent) from a
/// [template](crate::account::AccountComponentTemplate).
///
/// An [`InitStorageData`] can be created from a TOML string when the `std` feature flag is set.
#[derive(Clone, Debug, Default)]
pub struct InitStorageData {
    // TODO: Both the below fields could be a single field with a two variant enum
    // (eg, BTreeMap<StorageValueName, StorageTemplateValue>)
    /// A mapping of storage placeholder names to their corresponding storage values.
    value_entries: BTreeMap<StorageValueName, String>,
    /// A mapping of map placeholder names to their corresponding key/value entries.
    map_entries: BTreeMap<StorageValueName, Vec<(Word, Word)>>,
}

impl InitStorageData {
    /// Creates a new instance of [InitStorageData].
    ///
    /// A [`BTreeMap`] is constructed from the passed iterator, so duplicate keys will cause
    /// overridden values.
    ///
    /// # Parameters
    ///
    /// - `entries`: An iterable collection of key-value pairs.
    pub fn new(entries: impl IntoIterator<Item = (StorageValueName, String)>) -> Self {
        let value_entries = entries
            .into_iter()
            .filter(|(entry_name, _)| !entry_name.as_str().is_empty())
            .collect::<BTreeMap<_, _>>();

        InitStorageData::from_parts(value_entries, BTreeMap::new())
    }

    /// Creates a new instance from the provided placeholder and map data.
    pub(crate) fn from_parts(
        storage_placeholders: BTreeMap<StorageValueName, String>,
        map_entries: BTreeMap<StorageValueName, Vec<(Word, Word)>>,
    ) -> Self {
        InitStorageData {
            value_entries: storage_placeholders,
            map_entries,
        }
    }

    /// Retrieves a reference to the storage placeholders.
    pub fn placeholders(&self) -> &BTreeMap<StorageValueName, String> {
        &self.value_entries
    }

    /// Returns a reference to the name corresponding to the placeholder, or
    /// [`Option::None`] if the placeholder is not present.
    pub fn get(&self, key: &StorageValueName) -> Option<&String> {
        self.value_entries.get(key)
    }

    /// Returns the map entries associated with the given placeholder name, if any.
    pub(crate) fn map_entries(&self, key: &StorageValueName) -> Option<&Vec<(Word, Word)>> {
        self.map_entries.get(key)
    }
}

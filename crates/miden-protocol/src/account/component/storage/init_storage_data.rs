use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use thiserror::Error;

use super::StorageValueName;
use crate::account::StorageSlotName;
use crate::{Felt, FieldElement, Word};

/// A word value provided via [`InitStorageData`].
///
/// This is used for defining specific values in relation to a component's schema, where each value
/// is supplied as either a fully-typed word, an atomic string (e.g. `"0x1234"`, `"16"`, `"BTC"`),
/// or an array of 4 field elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WordValue {
    /// A fully-typed word value.
    FullyTyped(Word),
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

impl From<Word> for WordValue {
    fn from(value: Word) -> Self {
        WordValue::FullyTyped(value)
    }
}

impl From<Felt> for WordValue {
    /// Converts a [`Felt`] to a [`WordValue`] as a Word in the form `[0, 0, 0, felt]`.
    fn from(value: Felt) -> Self {
        WordValue::FullyTyped(Word::from([Felt::ZERO, Felt::ZERO, Felt::ZERO, value]))
    }
}

impl From<[Felt; 4]> for WordValue {
    fn from(value: [Felt; 4]) -> Self {
        WordValue::FullyTyped(Word::from(value))
    }
}

// INIT STORAGE DATA
// ====================================================================================================

/// Represents the data required to initialize storage entries when instantiating an
/// [AccountComponent](crate::account::AccountComponent) from component metadata (either provided
/// directly or extracted from a package).
///
/// An [`InitStorageData`] can be created from a TOML string when the `std` feature flag is set.
#[derive(Clone, Debug, Default)]
pub struct InitStorageData {
    /// A mapping of storage value names to their init values.
    value_entries: BTreeMap<StorageValueName, WordValue>,
    /// A mapping of storage map slot names to their init key/value entries.
    map_entries: BTreeMap<StorageSlotName, Vec<(WordValue, WordValue)>>,
}

impl InitStorageData {
    /// Returns a reference to the underlying init values map.
    pub fn values(&self) -> &BTreeMap<StorageValueName, WordValue> {
        &self.value_entries
    }

    /// Returns a reference to the underlying init map entries.
    pub fn maps(&self) -> &BTreeMap<StorageSlotName, Vec<(WordValue, WordValue)>> {
        &self.map_entries
    }

    /// Returns a reference to the stored init value for the given name.
    pub fn value_entry(&self, name: &StorageValueName) -> Option<&WordValue> {
        self.value_entries.get(name)
    }

    /// Returns a reference to the stored init value for a full slot name.
    pub fn slot_value_entry(&self, slot_name: &StorageSlotName) -> Option<&WordValue> {
        let name = StorageValueName::from_slot_name(slot_name);
        self.value_entries.get(&name)
    }

    /// Returns the map entries associated with the given storage map slot name, if any.
    pub fn map_entries(&self, slot_name: &StorageSlotName) -> Option<&Vec<(WordValue, WordValue)>> {
        self.map_entries.get(slot_name)
    }

    /// Merges another [`InitStorageData`] into this one, overwriting value entries and appending
    /// map entries.
    pub fn merge_from(&mut self, other: InitStorageData) {
        self.value_entries.extend(other.value_entries);
        for (slot_name, entries) in other.map_entries {
            self.map_entries.entry(slot_name).or_default().extend(entries);
        }
    }

    /// Returns true if any init value entry targets the given slot name.
    pub fn has_value_entries_for_slot(&self, slot_name: &StorageSlotName) -> bool {
        self.value_entries.keys().any(|name| name.slot_name() == slot_name)
    }

    /// Returns true if any init value entry targets a field of the given slot name.
    pub fn has_field_entries_for_slot(&self, slot_name: &StorageSlotName) -> bool {
        self.value_entries
            .keys()
            .any(|name| name.slot_name() == slot_name && name.field_name().is_some())
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Inserts a value entry, returning an error on duplicate or conflicting keys.
    ///
    /// The value can be any type that implements `Into<WordValue>`, e.g.:
    ///
    /// - `Word`: a fully-typed word value
    /// - `[Felt; 4]`: converted to a Word
    /// - `Felt`: converted to `[0, 0, 0, felt]`
    /// - `String` or `&str`: a parseable string value
    /// - `WordValue`: a raw or fully-typed word value
    pub fn insert_value(
        &mut self,
        name: StorageValueName,
        value: impl Into<WordValue>,
    ) -> Result<(), InitStorageDataError> {
        if self.value_entries.contains_key(&name) {
            return Err(InitStorageDataError::DuplicateKey(name.to_string()));
        }
        if self.map_entries.contains_key(name.slot_name()) {
            return Err(InitStorageDataError::ConflictingEntries(name.slot_name().as_str().into()));
        }
        self.value_entries.insert(name, value.into());
        Ok(())
    }

    /// Inserts a single map entry, returning an error on duplicate or conflicting keys.
    ///
    /// See [`Self::insert_value`] for examples of supported types for `key` and `value`.
    pub fn insert_map_entry(
        &mut self,
        slot_name: StorageSlotName,
        key: impl Into<WordValue>,
        value: impl Into<WordValue>,
    ) -> Result<(), InitStorageDataError> {
        if self.has_value_entries_for_slot(&slot_name) {
            return Err(InitStorageDataError::ConflictingEntries(slot_name.as_str().into()));
        }

        let key = key.into();
        if let Some(entries) = self.map_entries.get(&slot_name)
            && entries.iter().any(|(existing_key, _)| existing_key == &key)
        {
            return Err(InitStorageDataError::DuplicateKey(format!(
                "{}[{key:?}]",
                slot_name.as_str()
            )));
        }

        self.map_entries.entry(slot_name).or_default().push((key, value.into()));
        Ok(())
    }

    /// Sets map entries for the slot, replacing any existing entries.
    ///
    /// Returns an error if there are conflicting value entries.
    pub fn set_map_values(
        &mut self,
        slot_name: StorageSlotName,
        entries: Vec<(WordValue, WordValue)>,
    ) -> Result<(), InitStorageDataError> {
        if self.has_value_entries_for_slot(&slot_name) {
            return Err(InitStorageDataError::ConflictingEntries(slot_name.as_str().into()));
        }
        self.map_entries.insert(slot_name, entries);
        Ok(())
    }

    /// Sets a value entry, overriding any existing entry for the name.
    ///
    /// Returns an error if the [`StorageValueName`] has been used for a map slot.
    pub fn set_value(
        &mut self,
        name: StorageValueName,
        value: impl Into<WordValue>,
    ) -> Result<(), InitStorageDataError> {
        if self.map_entries.contains_key(name.slot_name()) {
            return Err(InitStorageDataError::ConflictingEntries(name.slot_name().as_str().into()));
        }
        self.value_entries.insert(name, value.into());
        Ok(())
    }
}

// ERRORS
// ====================================================================================================

/// Error returned when creating [`InitStorageData`] with invalid entries.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum InitStorageDataError {
    #[error("duplicate init key `{0}`")]
    DuplicateKey(String),
    #[error("conflicting init entries for `{0}`")]
    ConflictingEntries(String),
}

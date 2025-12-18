use alloc::string::{String, ToString};
use core::fmt::{self, Display};
use core::str::FromStr;

use miden_core::utils::{ByteReader, ByteWriter, Deserializable, Serializable};
use miden_processor::DeserializationError;
use thiserror::Error;

use crate::account::StorageSlotName;

/// A simple wrapper type around a string key that identifies init-provided values.
///
/// A storage value name is a string that identifies values supplied during component
/// instantiation (via [`InitStorageData`](super::InitStorageData)).
///
/// Names can be chained together to form unique keys for nested schema fields (e.g. a composed
/// word slot with per-felt typed fields).
#[derive(Clone, Debug, Ord, PartialOrd, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(::serde::Deserialize, ::serde::Serialize))]
#[cfg_attr(feature = "std", serde(transparent))]
pub struct StorageValueName {
    fully_qualified_name: String,
}

impl StorageValueName {
    /// Creates a [`StorageValueName`] for the given storage slot.
    ///
    /// This is an infallible conversion: a [`StorageSlotName`] is always a valid storage value
    /// name prefix.
    pub fn from_slot_name(slot_name: &StorageSlotName) -> Self {
        StorageValueName {
            fully_qualified_name: slot_name.as_str().to_string(),
        }
    }

    /// Creates an empty [`StorageValueName`].
    pub(crate) fn empty() -> Self {
        StorageValueName { fully_qualified_name: String::default() }
    }

    /// Adds a suffix to the storage value name, separated by a period.
    #[must_use]
    pub fn with_suffix(self, suffix: &StorageValueName) -> StorageValueName {
        let mut key = self;
        if !suffix.as_str().is_empty() {
            if !key.as_str().is_empty() {
                key.fully_qualified_name.push('.');
            }
            key.fully_qualified_name.push_str(suffix.as_str());
        }

        key
    }

    /// Returns the fully qualified name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.fully_qualified_name
    }

    fn validate_segment(segment: &str) -> Result<(), StorageValueNameError> {
        if segment.is_empty() {
            return Err(StorageValueNameError::EmptySegment);
        }
        if let Some(offending_char) = segment
            .chars()
            .find(|&c| !(c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':'))
        {
            return Err(StorageValueNameError::InvalidCharacter {
                part: segment.to_string(),
                character: offending_char,
            });
        }

        Ok(())
    }
}

impl FromStr for StorageValueName {
    type Err = StorageValueNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        for segment in value.split('.') {
            Self::validate_segment(segment)?;
        }
        Ok(Self { fully_qualified_name: value.to_string() })
    }
}

impl From<&StorageSlotName> for StorageValueName {
    fn from(value: &StorageSlotName) -> Self {
        StorageValueName::from_slot_name(value)
    }
}

impl Display for StorageValueName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serializable for StorageValueName {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(&self.fully_qualified_name);
    }
}

impl Deserializable for StorageValueName {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let key: String = source.read()?;
        Ok(StorageValueName { fully_qualified_name: key })
    }
}

#[derive(Debug, Error)]
pub enum StorageValueNameError {
    #[error("key segment is empty")]
    EmptySegment,
    #[error("key segment '{part}' contains invalid character '{character}'")]
    InvalidCharacter { part: String, character: char },
}

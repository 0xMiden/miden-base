use alloc::borrow::Cow;
use alloc::string::{String, ToString};
use core::fmt::Display;

use miden_core::utils::hash_string_to_word;

use crate::account::StorageValueName;
use crate::account::storage::slot::StorageSlotId;
use crate::errors::{AccountComponentTemplateError, SlotNameError};
use crate::utils::serde::{ByteWriter, Deserializable, DeserializationError, Serializable};

/// The name of an account storage slot.
///
/// A typical slot name looks like this:
///
/// ```text
/// miden::basic_fungible_faucet::metadata
/// ```
///
/// The double-colon (`::`) serves as a separator and the strings in between the separators are
/// called components.
///
/// It is generally recommended that slot names have at least three components and follow this
/// structure:
///
/// ```text
/// project_name::component_name::slot_name
/// ```
///
/// ## Requirements
///
/// For a string to be a valid slot name it needs to satisfy the following criteria:
/// - Its length must be less than 255.
/// - It needs to have at least 2 components.
/// - Each component must consist of at least one character.
/// - Each component must only consist of the characters `a` to `z`, `A` to `Z`, `0` to `9` or `_`
///   (underscore).
/// - Each component must not start with an underscore.
// TODO: Validate slot name during serde deserialization.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
pub struct StorageSlotName {
    name: Cow<'static, str>,
}

impl StorageSlotName {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The minimum number of components that a slot name must contain.
    pub(crate) const MIN_NUM_COMPONENTS: usize = 2;

    /// The maximum number of characters in a slot name.
    pub(crate) const MAX_LENGTH: usize = u8::MAX as usize;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Constructs a new [`StorageSlotName`] from a static string.
    ///
    /// This function is `const` and can be used to define slot names as constants, e.g.:
    ///
    /// ```rust
    /// # use miden_objects::account::StorageSlotName;
    /// const SLOT_NAME: StorageSlotName =
    ///     StorageSlotName::from_static_str("miden::basic_fungible_faucet::metadata");
    /// ```
    ///
    /// This is convenient because using a string that is not a valid slot name fails to compile.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - the slot name is invalid (see the type-level docs for the requirements).
    pub const fn from_static_str(name: &'static str) -> Self {
        match Self::validate(name) {
            Ok(()) => Self { name: Cow::Borrowed(name) },
            // We cannot format the error in a const context.
            Err(_) => panic!("invalid slot name"),
        }
    }

    /// Constructs a new [`StorageSlotName`] from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the slot name is invalid (see the type-level docs for the requirements).
    pub fn new(name: impl Into<String>) -> Result<Self, SlotNameError> {
        let name = name.into();
        Self::validate(&name)?;
        Ok(Self { name: Cow::Owned(name) })
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the slot name as a string slice.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Returns the slot name as a string slice.
    // allow is_empty to be missing because it would always return false since slot names are
    // enforced to have a length greater than zero, so it does not have much use.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u8 {
        // SAFETY: Slot name validation should enforce length fits into a u8.
        debug_assert!(self.name.len() <= Self::MAX_LENGTH);
        self.name.len() as u8
    }

    // TODO(named_slots): Docs.
    pub fn compute_id(&self) -> StorageSlotId {
        let hashed_word = hash_string_to_word(self.as_str());
        let suffix = hashed_word[0];
        let prefix = hashed_word[1];
        StorageSlotId::new(suffix, prefix)
    }

    // HELPERS
    // --------------------------------------------------------------------------------------------

    /// Validates a slot name.
    ///
    /// This checks that components are separated by double colons, that each component contains
    /// only valid characters and that the name is not empty or starts or ends with a colon.
    ///
    /// We must check the validity of a slot name against the raw bytes of the UTF-8 string because
    /// typical character APIs are not available in a const version. We can do this because any byte
    /// in a UTF-8 string that is an ASCII character never represents anything other than such a
    /// character, even though UTF-8 can contain multibyte sequences:
    ///
    /// > UTF-8, the object of this memo, has a one-octet encoding unit. It uses all bits of an
    /// > octet, but has the quality of preserving the full US-ASCII range: US-ASCII characters
    /// > are encoded in one octet having the normal US-ASCII value, and any octet with such a value
    /// > can only stand for a US-ASCII character, and nothing else.
    /// > https://www.rfc-editor.org/rfc/rfc3629
    const fn validate(name: &str) -> Result<(), SlotNameError> {
        let bytes = name.as_bytes();
        let mut idx = 0;
        let mut num_components = 0;

        if bytes.is_empty() {
            return Err(SlotNameError::TooShort);
        }

        if bytes.len() > Self::MAX_LENGTH {
            return Err(SlotNameError::TooLong);
        }

        // Slot names must not start with a colon or underscore.
        // SAFETY: We just checked that we're not dealing with an empty slice.
        if bytes[0] == b':' {
            return Err(SlotNameError::UnexpectedColon);
        } else if bytes[0] == b'_' {
            return Err(SlotNameError::UnexpectedUnderscore);
        }

        while idx < bytes.len() {
            let byte = bytes[idx];

            let is_colon = byte == b':';

            if is_colon {
                // A colon must always be followed by another colon. In other words, we
                // expect a double colon.
                if (idx + 1) < bytes.len() {
                    if bytes[idx + 1] != b':' {
                        return Err(SlotNameError::UnexpectedColon);
                    }
                } else {
                    return Err(SlotNameError::UnexpectedColon);
                }

                // A component cannot end with a colon, so this allows us to validate the start of a
                // component: It must not start with a colon or an underscore.
                if (idx + 2) < bytes.len() {
                    if bytes[idx + 2] == b':' {
                        return Err(SlotNameError::UnexpectedColon);
                    } else if bytes[idx + 2] == b'_' {
                        return Err(SlotNameError::UnexpectedUnderscore);
                    }
                } else {
                    return Err(SlotNameError::UnexpectedColon);
                }

                // Advance past the double colon.
                idx += 2;

                // A double colon completes a slot name component.
                num_components += 1;
            } else if Self::is_valid_char(byte) {
                idx += 1;
            } else {
                return Err(SlotNameError::InvalidCharacter);
            }
        }

        // The last component is not counted as part of the loop because no double colon follows.
        num_components += 1;

        if num_components < Self::MIN_NUM_COMPONENTS {
            return Err(SlotNameError::TooShort);
        }

        Ok(())
    }

    /// Returns `true` if the given byte is a valid slot name character, `false` otherwise.
    const fn is_valid_char(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }
}

impl Ord for StorageSlotName {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // TODO(named_slots): Cache ID in SlotName for efficiency.
        self.compute_id().cmp(&other.compute_id())
    }
}

impl PartialOrd for StorageSlotName {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for StorageSlotName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serializable for StorageSlotName {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write_u8(self.len());
        target.write_many(self.as_str().as_bytes())
    }

    fn get_size_hint(&self) -> usize {
        // Slot name length + slot name bytes
        1 + self.as_str().len()
    }
}

impl Deserializable for StorageSlotName {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, DeserializationError> {
        let len = source.read_u8()?;
        let name = source.read_many(len as usize)?;
        String::from_utf8(name)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))
            .and_then(|name| {
                Self::new(name).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
            })
    }
}

impl TryFrom<StorageValueName> for StorageSlotName {
    type Error = AccountComponentTemplateError;

    fn try_from(value_name: StorageValueName) -> Result<Self, Self::Error> {
        Ok(StorageSlotName::new(String::from(value_name)).expect("TODO: map error"))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use std::borrow::ToOwned;

    use assert_matches::assert_matches;

    use super::*;

    // A string containing all allowed characters of a slot name.
    const FULL_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz_ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";

    // Const function tests
    // --------------------------------------------------------------------------------------------

    const _NAME0: StorageSlotName = StorageSlotName::from_static_str("name::component");
    const _NAME1: StorageSlotName = StorageSlotName::from_static_str("one::two::three::four::five");
    const _NAME2: StorageSlotName = StorageSlotName::from_static_str("one::two_three::four");

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_on_invalid_character() {
        StorageSlotName::from_static_str("miden!::component");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_on_invalid_character2() {
        StorageSlotName::from_static_str("miden_ö::component");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_when_too_short() {
        StorageSlotName::from_static_str("one");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_on_component_starting_with_underscores() {
        StorageSlotName::from_static_str("one::_two");
    }

    // Invalid colon or underscore tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_fails_on_invalid_colon_placement() {
        // Single colon.
        assert_matches!(StorageSlotName::new(":").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(StorageSlotName::new("0::1:").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(StorageSlotName::new(":0::1").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(
            StorageSlotName::new("0::1:2").unwrap_err(),
            SlotNameError::UnexpectedColon
        );

        // Double colon (placed invalidly).
        assert_matches!(StorageSlotName::new("::").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(
            StorageSlotName::new("1::2::").unwrap_err(),
            SlotNameError::UnexpectedColon
        );
        assert_matches!(
            StorageSlotName::new("::1::2").unwrap_err(),
            SlotNameError::UnexpectedColon
        );

        // Triple colon.
        assert_matches!(StorageSlotName::new(":::").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(
            StorageSlotName::new("1::2:::").unwrap_err(),
            SlotNameError::UnexpectedColon
        );
        assert_matches!(
            StorageSlotName::new(":::1::2").unwrap_err(),
            SlotNameError::UnexpectedColon
        );
        assert_matches!(
            StorageSlotName::new("1::2:::3").unwrap_err(),
            SlotNameError::UnexpectedColon
        );
    }

    #[test]
    fn slot_name_fails_on_invalid_underscore_placement() {
        assert_matches!(
            StorageSlotName::new("_one::two").unwrap_err(),
            SlotNameError::UnexpectedUnderscore
        );
        assert_matches!(
            StorageSlotName::new("one::_two").unwrap_err(),
            SlotNameError::UnexpectedUnderscore
        );
    }

    // Length validation tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_fails_on_empty_string() {
        assert_matches!(StorageSlotName::new("").unwrap_err(), SlotNameError::TooShort);
    }

    #[test]
    fn slot_name_fails_on_single_component() {
        assert_matches!(
            StorageSlotName::new("single_component").unwrap_err(),
            SlotNameError::TooShort
        );
    }

    #[test]
    fn slot_name_fails_on_string_whose_length_exceeds_max_length() {
        let mut string = get_max_length_slot_name();
        string.push('a');
        assert_matches!(StorageSlotName::new(string).unwrap_err(), SlotNameError::TooLong);
    }

    // Alphabet validation tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_allows_ascii_alphanumeric_and_underscore() -> anyhow::Result<()> {
        let name = format!("{FULL_ALPHABET}::second");
        let slot_name = StorageSlotName::new(&name)?;
        assert_eq!(slot_name.as_str(), name);

        Ok(())
    }

    #[test]
    fn slot_name_fails_on_invalid_character() {
        assert_matches!(
            StorageSlotName::new("na#me::second").unwrap_err(),
            SlotNameError::InvalidCharacter
        );
        assert_matches!(
            StorageSlotName::new("first_entry::secönd").unwrap_err(),
            SlotNameError::InvalidCharacter
        );
        assert_matches!(
            StorageSlotName::new("first::sec::th!rd").unwrap_err(),
            SlotNameError::InvalidCharacter
        );
    }

    // Valid slot name tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_with_min_components_is_valid() -> anyhow::Result<()> {
        StorageSlotName::new("miden::component")?;
        Ok(())
    }

    #[test]
    fn slot_name_with_many_components_is_valid() -> anyhow::Result<()> {
        StorageSlotName::new("miden::faucet0::fungible_1::b4sic::metadata")?;
        Ok(())
    }

    #[test]
    fn slot_name_with_max_length_is_valid() -> anyhow::Result<()> {
        StorageSlotName::new(get_max_length_slot_name())?;
        Ok(())
    }

    // Serialization tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn serde_slot_name() -> anyhow::Result<()> {
        let slot_name = StorageSlotName::new("miden::faucet0::fungible_1::b4sic::metadata")?;
        assert_eq!(slot_name, StorageSlotName::read_from_bytes(&slot_name.to_bytes())?);
        Ok(())
    }

    #[test]
    fn serde_max_length_slot_name() -> anyhow::Result<()> {
        let slot_name = StorageSlotName::new(get_max_length_slot_name())?;
        assert_eq!(slot_name, StorageSlotName::read_from_bytes(&slot_name.to_bytes())?);
        Ok(())
    }

    // Test helpers
    // --------------------------------------------------------------------------------------------

    fn get_max_length_slot_name() -> String {
        const MIDEN_STR: &str = "miden::";
        let remainder = ['a'; StorageSlotName::MAX_LENGTH - MIDEN_STR.len()];
        let mut string = MIDEN_STR.to_owned();
        string.extend(remainder);
        assert_eq!(string.len(), StorageSlotName::MAX_LENGTH);
        string
    }
}

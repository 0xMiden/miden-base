use alloc::borrow::Cow;
use alloc::string::String;

use crate::errors::SlotNameError;

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
/// ## Requirements
///
/// For a string to be a valid slot name it needs to satisfy the following criteria:
/// - It needs to have at least 2 components.
/// - Each component must consist of at least one character.
/// - Each component must only consist of the characters `a` to `z`, `A` to `Z`, `0` to `9` or `_`
///   (underscore).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotName {
    name: Cow<'static, str>,
}

impl SlotName {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    // The minimum number of components that a slot name must contain.
    pub(crate) const MIN_NUM_COMPONENTS: usize = 2;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Constructs a new [`SlotName`] from a static string.
    ///
    /// This function is `const` and can be used to define slot names as constants, e.g.:
    ///
    /// ```rust
    /// # use miden_objects::account::SlotName;
    /// const SLOT_NAME: SlotName = SlotName::from_static_str("miden::basic_fungible_faucet::metadata");
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

    /// Constructs a new [`SlotName`] from a string.
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
    /// caracter, even though UTF-8 can contain multibyte sequences:
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
                // component: It must not not start with a colon or an underscore.
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
        b'A' <= byte && byte <= b'Z'
            || b'a' <= byte && byte <= b'z'
            || b'0' <= byte && byte <= b'9'
            || b'_' == byte
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    // A string containing all allowed characters of a slot name.
    const FULL_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz_ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";

    // Const function tests
    // --------------------------------------------------------------------------------------------

    const _NAME0: SlotName = SlotName::from_static_str("name::component");
    const _NAME1: SlotName = SlotName::from_static_str("one::two::three::four::five");
    const _NAME2: SlotName = SlotName::from_static_str("one::two_three::four");

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_on_invalid_character() {
        SlotName::from_static_str("miden!::component");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_on_invalid_character2() {
        SlotName::from_static_str("miden_ö::component");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_when_too_short() {
        SlotName::from_static_str("one");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_on_component_starting_with_underscores() {
        SlotName::from_static_str("one::_two");
    }

    // Invalid colon or underscore tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_fails_on_invalid_colon_placement() {
        // Single colon.
        assert_matches!(SlotName::new(":").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(SlotName::new("0::1:").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(SlotName::new(":0::1").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(SlotName::new("0::1:2").unwrap_err(), SlotNameError::UnexpectedColon);

        // Double colon (placed invalidly).
        assert_matches!(SlotName::new("::").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(SlotName::new("1::2::").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(SlotName::new("::1::2").unwrap_err(), SlotNameError::UnexpectedColon);

        // Triple colon.
        assert_matches!(SlotName::new(":::").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(SlotName::new("1::2:::").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(SlotName::new(":::1::2").unwrap_err(), SlotNameError::UnexpectedColon);
        assert_matches!(SlotName::new("1::2:::3").unwrap_err(), SlotNameError::UnexpectedColon);
    }

    #[test]
    fn slot_name_fails_on_invalid_underscore_placement() {
        assert_matches!(
            SlotName::new("_one::two").unwrap_err(),
            SlotNameError::UnexpectedUnderscore
        );
        assert_matches!(
            SlotName::new("one::_two").unwrap_err(),
            SlotNameError::UnexpectedUnderscore
        );
    }

    // Num components tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_fails_on_empty_string() {
        assert_matches!(SlotName::new("").unwrap_err(), SlotNameError::TooShort);
    }

    #[test]
    fn slot_name_fails_on_single_component() {
        assert_matches!(SlotName::new("single_component").unwrap_err(), SlotNameError::TooShort);
    }

    // Alphabet validation tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_allows_ascii_alphanumeric_and_underscore() -> anyhow::Result<()> {
        let name = format!("{FULL_ALPHABET}::second");
        let slot_name = SlotName::new(&name)?;
        assert_eq!(slot_name.as_str(), name);

        Ok(())
    }

    #[test]
    fn slot_name_fails_on_invalid_character() {
        assert_matches!(
            SlotName::new("na#me::second").unwrap_err(),
            SlotNameError::InvalidCharacter
        );
        assert_matches!(
            SlotName::new("first_entry::secönd").unwrap_err(),
            SlotNameError::InvalidCharacter
        );
        assert_matches!(
            SlotName::new("first::sec::th!rd").unwrap_err(),
            SlotNameError::InvalidCharacter
        );
    }

    // Valid slot name tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_with_min_components_is_valid() -> anyhow::Result<()> {
        SlotName::new("miden::component")?;
        Ok(())
    }

    #[test]
    fn slot_name_with_many_components_is_valid() -> anyhow::Result<()> {
        SlotName::new("miden::faucet0::fungible_1::b4sic::metadata")?;
        Ok(())
    }
}

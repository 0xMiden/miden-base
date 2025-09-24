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
/// - It needs to have at most 5 components.
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

    pub(crate) const MIN_COMPONENTS: usize = 2;
    pub(crate) const MAX_COMPONENTS: usize = 5;

    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Constructs a new [`SlotName`] from a static string.
    ///
    /// This function is `const` and can be used to define slot names as constants, e.g.:
    ///
    /// ```rust
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
    /// > UTF-8, the object of this memo, has a one-octet encoding unit. It uses all bits of an
    /// > octet, but has the quality of preserving the full US-ASCII range: US-ASCII characters
    /// > are encoded in one octet having the normal US-ASCII value, and any octet with such a value
    /// > can only stand for a US-ASCII character, and nothing else.
    /// > https://www.rfc-editor.org/rfc/rfc3629
    ///
    /// Because of the above, we can check the validity of a slot name against the raw bytes of
    /// the UTF-8 string, which allows us to do this in a const fn. Any byte we check against
    /// here must be an ASCII value or otherwise represent an invalid character.
    const fn validate(name: &str) -> Result<(), SlotNameError> {
        let components = match Self::components(name) {
            Ok(components) => components,
            Err(err) => return Err(err),
        };

        if components[Self::MIN_COMPONENTS - 1].is_none() {
            return Err(SlotNameError::TooShort);
        }

        let mut component_idx = 0;
        while component_idx < components.len() {
            let component = match components[component_idx] {
                Some(component) => component,
                None => break,
            };

            if let Err(err) = Self::validate_component(component) {
                return Err(err);
            }

            component_idx += 1;
        }

        Ok(())
    }

    /// Validates a single component of a slot name.
    ///
    /// This checks that each of the component's characters is in the allowed alphabet.
    const fn validate_component(component: &str) -> Result<(), SlotNameError> {
        let bytes = component.as_bytes();
        let mut idx = 0;

        // A while loop is necessary for the function to be const.
        while idx < bytes.len() {
            let byte = bytes[idx];

            let is_valid_char = b'A' <= byte && byte <= b'Z'
                || b'a' <= byte && byte <= b'z'
                || b'0' <= byte && byte <= b'9'
                || b'_' == byte;

            if !is_valid_char {
                return Err(SlotNameError::InvalidAlphabet);
            }

            idx += 1;
        }

        Ok(())
    }

    /// Extracts the components from a slot name.
    ///
    /// This checks that each component is separated from another component by a double colon and
    /// that components aren't empty.
    const fn components(name: &str) -> Result<[Option<&str>; Self::MAX_COMPONENTS], SlotNameError> {
        const fn finalize_component(
            bytes: &[u8],
            idx: usize,
        ) -> Result<(&str, &[u8]), SlotNameError> {
            let (component_slice, mut remainder_slice) = bytes.split_at(idx);
            let component = match str::from_utf8(component_slice) {
                Ok(component) => component,
                Err(_) => return Err(SlotNameError::InvalidAlphabet),
            };

            // If there is a non-empty remainder, advance it past the double colon, i.e. skip two
            // bytes.
            if !remainder_slice.is_empty() {
                let (_double_colon_slice, remainder) = remainder_slice.split_at(2);
                debug_assert!(_double_colon_slice[0] == b':');
                debug_assert!(_double_colon_slice[1] == b':');

                remainder_slice = remainder;
            }

            // A component's minimum length is 1. If we encountered a colon when the length
            // is 0, the colon must be invalidly placed.
            if component.is_empty() {
                return Err(SlotNameError::InvalidColon);
            }

            Ok((component, remainder_slice))
        }

        let mut components = [None; Self::MAX_COMPONENTS];
        let mut component_idx = 0;

        let mut bytes = name.as_bytes();
        let mut idx = 0;

        while idx < bytes.len() {
            let byte = bytes[idx];

            // A colon must always be followed by another colon. In other words, we
            // expect a double colon.
            let is_colon = byte == b':';

            if is_colon {
                let is_followed_by_colon = if (idx + 1) < bytes.len() {
                    bytes[idx + 1] == b':'
                } else {
                    return Err(SlotNameError::InvalidColon);
                };

                if !is_followed_by_colon {
                    return Err(SlotNameError::InvalidColon);
                }

                let (component, remainder_slice) = match finalize_component(bytes, idx) {
                    Ok(r) => r,
                    Err(err) => return Err(err),
                };

                // Store the the current component.
                components[component_idx] = Some(component);
                component_idx += 1;

                bytes = remainder_slice;
                idx = 0;
            } else {
                idx += 1;
            }
        }

        let (component, remainder_slice) = match finalize_component(bytes, idx) {
            Ok(r) => r,
            Err(err) => return Err(err),
        };
        debug_assert!(remainder_slice.is_empty());

        if component_idx >= Self::MAX_COMPONENTS {
            return Err(SlotNameError::TooLong);
        }

        components[component_idx] = Some(component);

        Ok(components)
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;

    use super::*;

    const FULL_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789";

    // Const function tests
    // --------------------------------------------------------------------------------------------

    const _NAME0: SlotName = SlotName::from_static_str("name::component");
    const _NAME1: SlotName = SlotName::from_static_str("one::two::three::four::five");

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_on_invalid_alphabet() {
        SlotName::from_static_str("miden!::component");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_on_invalid_alphabet2() {
        SlotName::from_static_str("miden_ö::component");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_when_too_short() {
        SlotName::from_static_str("one");
    }

    #[test]
    #[should_panic(expected = "invalid slot name")]
    fn slot_name_panics_when_too_long() {
        SlotName::from_static_str("one::two::three::four::five::six");
    }

    // Invalid colon placement tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_fails_on_invalid_colon_placement() {
        // Single colon.
        assert_matches!(SlotName::components(":").unwrap_err(), SlotNameError::InvalidColon);
        assert_matches!(SlotName::components("n:").unwrap_err(), SlotNameError::InvalidColon);
        assert_matches!(SlotName::components(":n").unwrap_err(), SlotNameError::InvalidColon);
        assert_matches!(SlotName::components("n:c").unwrap_err(), SlotNameError::InvalidColon);

        // Double colon (placed invalidly).
        assert_matches!(SlotName::components("::").unwrap_err(), SlotNameError::InvalidColon);
        assert_matches!(SlotName::components("n::").unwrap_err(), SlotNameError::InvalidColon);
        assert_matches!(SlotName::components("::n").unwrap_err(), SlotNameError::InvalidColon);

        // Triple colon.
        assert_matches!(SlotName::components(":::").unwrap_err(), SlotNameError::InvalidColon);
        assert_matches!(SlotName::components("n:::").unwrap_err(), SlotNameError::InvalidColon);
        assert_matches!(SlotName::components(":::n").unwrap_err(), SlotNameError::InvalidColon);
        assert_matches!(SlotName::components("n:::c").unwrap_err(), SlotNameError::InvalidColon);
    }

    // Num components tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_fails_when_too_short() {
        assert_matches!(SlotName::new("single_component").unwrap_err(), SlotNameError::TooShort);
    }

    #[test]
    fn slot_name_fails_when_too_long() {
        let names = ["name"; SlotName::MAX_COMPONENTS + 1];
        let name = names.join("::");

        assert_matches!(SlotName::new(name).unwrap_err(), SlotNameError::TooLong);
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
    fn slot_name_fails_on_invalid_alphabet() {
        assert_matches!(
            SlotName::new("na#me::second").unwrap_err(),
            SlotNameError::InvalidAlphabet
        );
        assert_matches!(
            SlotName::new("first_entry::secönd").unwrap_err(),
            SlotNameError::InvalidAlphabet
        );
        assert_matches!(
            SlotName::new("first::sec::th!rd").unwrap_err(),
            SlotNameError::InvalidAlphabet
        );
    }

    // Name component tests
    // --------------------------------------------------------------------------------------------

    #[test]
    fn slot_name_min_components() -> anyhow::Result<()> {
        let miden = "miden";
        let component = "component";

        let name = format!("{miden}::{component}");
        let components = SlotName::components(&name)?;

        assert_eq!(components[0], Some(miden));
        assert_eq!(components[1], Some(component));
        assert_eq!(components[2], None);
        assert_eq!(components[3], None);
        assert_eq!(components[4], None);

        Ok(())
    }

    #[test]
    fn slot_name_max_components() -> anyhow::Result<()> {
        let one = "o1ne";
        let two = "2two";
        let three = "three33";
        let four = "four";
        let five = "five5";

        let name = format!("{one}::{two}::{three}::{four}::{five}");
        let components = SlotName::components(&name)?;

        assert_eq!(components[0], Some(one));
        assert_eq!(components[1], Some(two));
        assert_eq!(components[2], Some(three));
        assert_eq!(components[3], Some(four));
        assert_eq!(components[4], Some(five));

        Ok(())
    }
}

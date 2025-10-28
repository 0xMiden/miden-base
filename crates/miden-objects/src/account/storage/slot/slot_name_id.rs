use core::cmp::Ordering;
use core::fmt::Display;

use crate::Felt;

/// The identifier of a [`SlotName`](super::SlotName).
///
/// The ID of a slot name are the first two felts of the blake3-hashed slot name. The suffix is the
/// 0th element and the prefix is the 1st element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotNameId {
    prefix: Felt,
    suffix: Felt,
}

impl SlotNameId {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`SlotNameId`] from the provided felts.
    pub fn new(prefix: Felt, suffix: Felt) -> Self {
        Self { prefix, suffix }
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the prefix of the [`SlotNameId`].
    pub fn prefix(&self) -> Felt {
        self.prefix
    }

    /// Returns the suffix of the [`SlotNameId`].
    pub fn suffix(&self) -> Felt {
        self.suffix
    }

    /// Returns the [`SlotNameId`]'s felts encoded into a u128.
    fn as_u128(&self) -> u128 {
        let mut le_bytes = [0_u8; 16];
        le_bytes[..8].copy_from_slice(&self.suffix().as_int().to_le_bytes());
        le_bytes[8..].copy_from_slice(&self.prefix().as_int().to_le_bytes());
        u128::from_le_bytes(le_bytes)
    }
}

impl Ord for SlotNameId {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.prefix.as_int().cmp(&other.prefix.as_int()) {
            ord @ Ordering::Less | ord @ Ordering::Greater => ord,
            Ordering::Equal => self.suffix.as_int().cmp(&other.suffix.as_int()),
        }
    }
}

impl PartialOrd for SlotNameId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for SlotNameId {
    /// Returns a big-endian, hex-encoded string of length 34, including the `0x` prefix.
    ///
    /// This means it encodes 16 bytes.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("0x{:032x}", self.as_u128()))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_id_as_u128() {
        let prefix = 1;
        let suffix = 5;
        let name_id = SlotNameId::new(Felt::from(prefix as u32), Felt::from(suffix as u32));
        assert_eq!(name_id.as_u128(), (prefix << 64) + suffix);
        assert_eq!(format!("{name_id}"), "0x00000000000000010000000000000005");
    }
}

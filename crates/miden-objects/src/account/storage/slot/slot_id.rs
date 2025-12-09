use core::cmp::Ordering;
use core::fmt::Display;

use crate::Felt;

/// The partial hash of a [`SlotName`](super::SlotName).
///
/// The ID of a slot are the first (`suffix`) and second (`prefix`) field elements of the
/// blake3-hashed slot name.
///
/// The slot ID is used to uniquely identify a storage slot and is used to sort slots in account
/// storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotId {
    suffix: Felt,
    prefix: Felt,
}

impl SlotId {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`SlotId`] from the provided felts.
    pub fn new(suffix: Felt, prefix: Felt) -> Self {
        Self { suffix, prefix }
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the suffix of the [`SlotId`].
    pub fn suffix(&self) -> Felt {
        self.suffix
    }

    /// Returns the prefix of the [`SlotId`].
    pub fn prefix(&self) -> Felt {
        self.prefix
    }

    /// Returns the [`SlotId`]'s felts encoded into a u128.
    fn as_u128(&self) -> u128 {
        let mut le_bytes = [0_u8; 16];
        le_bytes[..8].copy_from_slice(&self.suffix().as_int().to_le_bytes());
        le_bytes[8..].copy_from_slice(&self.prefix().as_int().to_le_bytes());
        u128::from_le_bytes(le_bytes)
    }
}

impl Ord for SlotId {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.prefix.as_int().cmp(&other.prefix.as_int()) {
            ord @ Ordering::Less | ord @ Ordering::Greater => ord,
            Ordering::Equal => self.suffix.as_int().cmp(&other.suffix.as_int()),
        }
    }
}

impl PartialOrd for SlotId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for SlotId {
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
    fn test_slot_id_as_u128() {
        let suffix = 5;
        let prefix = 3;
        let name_id = SlotId::new(Felt::from(suffix as u32), Felt::from(prefix as u32));
        assert_eq!(name_id.as_u128(), (prefix << 64) + suffix);
        assert_eq!(format!("{name_id}"), "0x00000000000000030000000000000005");
    }
}

use crate::AddressError;
use crate::errors::Bech32Error;

/// The type of an [`Address`](super::Address) in Miden.
///
/// The byte values of this address type should be chosen as a multiple of 8. That way, the first
/// character of the bech32 string after the `1` separator will be different for every address type.
/// This makes the type of the address conveniently human-readable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
#[non_exhaustive]
pub enum AddressType {
    AccountId = Self::ACCOUNT_ID,
}

impl AddressType {
    // Constants for internal use only.
    const ACCOUNT_ID: u8 = 0;
}

impl TryFrom<u8> for AddressType {
    type Error = AddressError;

    /// Decodes an [`AddressType`] from its byte representation.
    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            Self::ACCOUNT_ID => Ok(Self::AccountId),
            other => Err(AddressError::Bech32DecodeError(Bech32Error::UnknownAddressType(other))),
        }
    }
}

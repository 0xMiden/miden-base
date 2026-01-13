use alloc::format;
use alloc::string::{String, ToString};
use core::fmt;

use miden_core::FieldElement;
use miden_protocol::Felt;
use miden_protocol::account::AccountId;
use miden_protocol::utils::{HexParseError, bytes_to_hex_string, hex_to_bytes};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddrConvError {
    NonZeroWordPadding,
    NonZeroBytePrefix,
    InvalidHexLength,
    InvalidHexChar(char),
    HexParseError,
    FeltOutOfField,
    InvalidAccountId,
}

impl fmt::Display for AddrConvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddrConvError::NonZeroWordPadding => write!(f, "non-zero word padding"),
            AddrConvError::NonZeroBytePrefix => write!(f, "address has non-zero 4-byte prefix"),
            AddrConvError::InvalidHexLength => {
                write!(f, "invalid hex length (expected 40 hex chars)")
            },
            AddrConvError::InvalidHexChar(c) => write!(f, "invalid hex character: {}", c),
            AddrConvError::HexParseError => write!(f, "hex parse error"),
            AddrConvError::FeltOutOfField => {
                write!(f, "packed 64-bit word does not fit in the field")
            },
            AddrConvError::InvalidAccountId => write!(f, "invalid AccountId"),
        }
    }
}

impl From<HexParseError> for AddrConvError {
    fn from(_err: HexParseError) -> Self {
        AddrConvError::HexParseError
    }
}

// ================================================================================================
// ETHEREUM ADDRESS
// ================================================================================================

/// Represents an Ethereum address format (20 bytes).
///
/// # Representations used in this module
///
/// - Raw bytes: `[u8; 20]` in the conventional Ethereum big-endian byte order (`bytes[0]` is the
///   most-significant byte).
/// - MASM "address\[5\]" limbs: 5 x u32 limbs in *little-endian limb order*:
///   - addr0 = bytes[16..19] (least-significant 4 bytes)
///   - addr1 = bytes[12..15]
///   - addr2 = bytes[ 8..11]
///   - addr3 = bytes[ 4.. 7]
///   - addr4 = bytes[ 0.. 3] (most-significant 4 bytes)
/// - Embedded AccountId format: `0x00000000 || prefix(8) || suffix(8)`, where:
///   - prefix = (addr3 << 32) | addr2 = bytes[4..11] as a big-endian u64
///   - suffix = (addr1 << 32) | addr0 = bytes[12..19] as a big-endian u64
///
/// Note: prefix/suffix are *conceptual* 64-bit words; when converting to [`Felt`], we must ensure
/// `Felt::new(u64)` does not reduce mod p (checked explicitly in `to_account_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EthAddressFormat([u8; 20]);

impl EthAddressFormat {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`EthAddressFormat`] from a 20-byte array.
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Creates an [`EthAddressFormat`] from a hex string (with or without "0x" prefix).
    ///
    /// # Errors
    ///
    /// Returns an error if the hex string is invalid or the hex part is not exactly 40 characters.
    pub fn from_hex(hex_str: &str) -> Result<Self, AddrConvError> {
        let hex_part = hex_str.strip_prefix("0x").unwrap_or(hex_str);
        if hex_part.len() != 40 {
            return Err(AddrConvError::InvalidHexLength);
        }

        let prefixed_hex = if hex_str.starts_with("0x") {
            hex_str.to_string()
        } else {
            format!("0x{}", hex_str)
        };

        let bytes: [u8; 20] = hex_to_bytes(&prefixed_hex)?;
        Ok(Self(bytes))
    }

    /// Creates an [`EthAddressFormat`] from an [`AccountId`].
    ///
    /// This conversion is infallible: an [`AccountId`] is two felts, and `as_int()` yields `u64`
    /// words which we embed as `0x00000000 || prefix(8) || suffix(8)` (big-endian words).
    pub fn from_account_id(account_id: AccountId) -> Self {
        let felts: [Felt; 2] = account_id.into();

        let mut out = [0u8; 20];
        out[4..12].copy_from_slice(&felts[0].as_int().to_be_bytes());
        out[12..20].copy_from_slice(&felts[1].as_int().to_be_bytes());

        Self(out)
    }

    // CONVERSIONS
    // --------------------------------------------------------------------------------------------

    /// Returns the raw 20-byte array.
    pub const fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    /// Converts the address into a 20-byte array.
    pub const fn into_bytes(self) -> [u8; 20] {
        self.0
    }

    /// Converts the Ethereum address into an array of 5 [`Felt`] values.
    ///
    /// The returned order matches the MASM `address\[5\]` convention (*little-endian limb order*):
    /// - addr0 = bytes[16..19] (least-significant 4 bytes)
    /// - addr1 = bytes[12..15]
    /// - addr2 = bytes[ 8..11]
    /// - addr3 = bytes[ 4.. 7]
    /// - addr4 = bytes[ 0.. 3] (most-significant 4 bytes)
    ///
    /// Each limb is interpreted as a big-endian `u32` and stored in a [`Felt`].
    pub fn to_elements(&self) -> [Felt; 5] {
        let mut result = [Felt::ZERO; 5];

        // i=0 -> bytes[16..20], i=4 -> bytes[0..4]
        for (i, felt) in result.iter_mut().enumerate() {
            let start = (4 - i) * 4;
            let chunk = &self.0[start..start + 4];
            let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            *felt = Felt::new(value as u64);
        }

        result
    }

    /// Converts the Ethereum address to an [`AccountId`].
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the first 4 bytes are not zero (not in the embedded AccountId format),
    /// - packing the 8-byte prefix/suffix into [`Felt`] would reduce mod p,
    /// - or the resulting felts do not form a valid [`AccountId`].
    pub fn to_account_id(&self) -> Result<AccountId, AddrConvError> {
        let (prefix, suffix) = Self::bytes20_to_prefix_suffix(self.0)?;

        // `Felt::new(u64)` may reduce mod p for some u64 values. Mirror the MASM `build_felt`
        // safety: construct the felt, then require round-trip equality.
        let prefix_felt = Felt::new(prefix);
        if prefix_felt.as_int() != prefix {
            return Err(AddrConvError::FeltOutOfField);
        }

        let suffix_felt = Felt::new(suffix);
        if suffix_felt.as_int() != suffix {
            return Err(AddrConvError::FeltOutOfField);
        }

        AccountId::try_from([prefix_felt, suffix_felt]).map_err(|_| AddrConvError::InvalidAccountId)
    }

    /// Converts the Ethereum address to a hex string (lowercase, 0x-prefixed).
    pub fn to_hex(&self) -> String {
        bytes_to_hex_string(self.0)
    }

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Convert `[u8; 20]` -> `(prefix, suffix)` by extracting the last 16 bytes.
    /// Requires the first 4 bytes be zero.
    /// Returns prefix and suffix values that match the MASM little-endian limb implementation:
    /// - prefix = bytes[4..12] as big-endian u64 = (addr3 << 32) | addr2
    /// - suffix = bytes[12..20] as big-endian u64 = (addr1 << 32) | addr0
    fn bytes20_to_prefix_suffix(bytes: [u8; 20]) -> Result<(u64, u64), AddrConvError> {
        if bytes[0..4] != [0, 0, 0, 0] {
            return Err(AddrConvError::NonZeroBytePrefix);
        }

        let prefix = u64::from_be_bytes(bytes[4..12].try_into().unwrap());
        let suffix = u64::from_be_bytes(bytes[12..20].try_into().unwrap());

        Ok((prefix, suffix))
    }
}

impl fmt::Display for EthAddressFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl From<[u8; 20]> for EthAddressFormat {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl From<AccountId> for EthAddressFormat {
    fn from(account_id: AccountId) -> Self {
        EthAddressFormat::from_account_id(account_id)
    }
}

impl From<EthAddressFormat> for [u8; 20] {
    fn from(addr: EthAddressFormat) -> Self {
        addr.0
    }
}

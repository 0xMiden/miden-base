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
}

impl fmt::Display for AddrConvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddrConvError::HexParseError => write!(f, "Hex parse error"),
            _ => write!(f, "{:?}", self),
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

/// Represents an Ethereum address (20 bytes).
///
/// This type provides conversions between Ethereum addresses and Miden types such as
/// [`AccountId`] and field elements ([`Felt`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EthAddress([u8; 20]);

impl EthAddress {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`EthAddress`] from a 20-byte array.
    pub const fn new(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }

    /// Creates an [`EthAddress`] from a hex string (with or without "0x" prefix).
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

    /// Creates an [`EthAddress`] from an [`AccountId`].
    ///
    /// The AccountId is converted to an Ethereum address using the embedded format where
    /// the first 4 bytes are zero padding, followed by the prefix and suffix as u64 values
    /// in big-endian format.
    ///
    /// # Errors
    ///
    /// Returns an error if the conversion fails (e.g., if the AccountId cannot be represented
    /// as a valid Ethereum address).
    pub fn from_account_id(account_id: AccountId) -> Result<Self, AddrConvError> {
        let felts: [Felt; 2] = account_id.into();
        let words = [felts[0].as_int(), felts[1].as_int()];

        let mut out = [0u8; 20];
        let w0 = words[0].to_be_bytes();
        let w1 = words[1].to_be_bytes();

        out[0..4].copy_from_slice(&[0, 0, 0, 0]);
        out[4..12].copy_from_slice(&w0);
        out[12..20].copy_from_slice(&w1);

        Ok(Self(out))
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
    /// Each felt represents 4 bytes of the address in big-endian format.
    pub fn to_elements(&self) -> [Felt; 5] {
        let mut result = [Felt::ZERO; 5];
        for (i, felt) in result.iter_mut().enumerate() {
            let start = i * 4;
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
    /// Returns an error if the first 4 bytes are not zero or if the resulting
    /// AccountId is invalid.
    pub fn to_account_id(&self) -> Result<AccountId, AddrConvError> {
        let u64x5 = Self::bytes20_to_u64x5(self.0)?;
        let felts = [Felt::new(u64x5[0]), Felt::new(u64x5[1])];

        match AccountId::try_from(felts) {
            Ok(account_id) => Ok(account_id),
            Err(_) => Err(AddrConvError::NonZeroBytePrefix),
        }
    }

    /// Converts the Ethereum address to a hex string (lowercase, 0x-prefixed).
    pub fn to_hex(&self) -> String {
        bytes_to_hex_string(self.0)
    }

    // HELPER FUNCTIONS
    // --------------------------------------------------------------------------------------------

    /// Convert `[u8; 20]` -> `[u64; 5]` by extracting the last 16 bytes.
    /// Requires the first 4 bytes be zero.
    fn bytes20_to_u64x5(bytes: [u8; 20]) -> Result<[u64; 5], AddrConvError> {
        if bytes[0..4] != [0, 0, 0, 0] {
            return Err(AddrConvError::NonZeroBytePrefix);
        }

        let w0 = u64::from_be_bytes(bytes[4..12].try_into().unwrap());
        let w1 = u64::from_be_bytes(bytes[12..20].try_into().unwrap());

        Ok([w0, w1, 0, 0, 0])
    }
}

impl fmt::Display for EthAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl From<[u8; 20]> for EthAddress {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl From<EthAddress> for [u8; 20] {
    fn from(addr: EthAddress) -> Self {
        addr.0
    }
}

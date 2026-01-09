use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use miden_core::FieldElement;
use miden_protocol::Felt;
use miden_protocol::account::AccountId;

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
    /// Returns an error if the hex string is invalid or not 40 characters (20 bytes).
    pub fn from_hex(hex_str: &str) -> Result<Self, AddrConvError> {
        evm_hex_to_bytes20(hex_str).map(Self)
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
        account_id_to_ethereum_address(account_id).map(Self)
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

    /// Converts the Ethereum address into a vector of 5 [`Felt`] values.
    ///
    /// Each felt represents 4 bytes of the address in big-endian format.
    pub fn to_felts(&self) -> Vec<Felt> {
        self.0
            .chunks(4)
            .map(|chunk| {
                let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                Felt::new(value as u64)
            })
            .collect()
    }

    /// Converts the Ethereum address into an array of 5 [`Felt`] values.
    ///
    /// Each felt represents 4 bytes of the address in big-endian format.
    pub fn to_felt_array(&self) -> [Felt; 5] {
        let mut result = [Felt::ZERO; 5];
        for (i, chunk) in self.0.chunks(4).enumerate() {
            let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            result[i] = Felt::new(value as u64);
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
        ethereum_address_to_account_id(&self.0)
    }

    /// Converts the Ethereum address to a hex string (lowercase, 0x-prefixed).
    pub fn to_hex(&self) -> String {
        bytes20_to_evm_hex(self.0)
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

// ================================================================================================
// UTILITY FUNCTIONS
// ================================================================================================

/// Converts a bytes32 value (32 bytes) into a vector of 8 Felt values.
pub fn bytes32_to_felts(bytes32: &[u8; 32]) -> Vec<Felt> {
    bytes32
        .chunks(4)
        .map(|chunk| {
            let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            Felt::new(value as u64)
        })
        .collect()
}

/// Convert 8 Felt values (u32 limbs in little-endian order) to U256 bytes in little-endian format.
pub fn felts_to_u256_bytes(limbs: [Felt; 8]) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    for (i, limb) in limbs.iter().enumerate() {
        let u32_value = limb.as_int() as u32;
        let limb_bytes = u32_value.to_le_bytes();
        bytes[i * 4..(i + 1) * 4].copy_from_slice(&limb_bytes);
    }
    bytes
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddrConvError {
    NonZeroWordPadding,
    NonZeroBytePrefix,
    InvalidHexLength,
    InvalidHexChar(char),
}

impl fmt::Display for AddrConvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Convert `[u64; 5]` -> `[u8; 20]` (EVM address bytes).
/// Layout: 4 zero bytes prefix + word0(be) + word1(be)
pub fn u64x5_to_bytes20(words: [u64; 5]) -> Result<[u8; 20], AddrConvError> {
    if words[2] != 0 || words[3] != 0 || words[4] != 0 {
        return Err(AddrConvError::NonZeroWordPadding);
    }

    let mut out = [0u8; 20];
    let w0 = words[0].to_be_bytes();
    let w1 = words[1].to_be_bytes();

    out[0..4].copy_from_slice(&[0, 0, 0, 0]);
    out[4..12].copy_from_slice(&w0);
    out[12..20].copy_from_slice(&w1);

    Ok(out)
}

/// Convert `[u8; 20]` -> EVM address hex string (lowercase, 0x-prefixed).
pub(crate) fn bytes20_to_evm_hex(bytes: [u8; 20]) -> String {
    let mut s = String::with_capacity(42);
    s.push_str("0x");
    for b in bytes {
        s.push(nibble_to_hex(b >> 4));
        s.push(nibble_to_hex(b & 0x0f));
    }
    s
}

fn nibble_to_hex(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + (n - 10)) as char,
        _ => unreachable!(),
    }
}

/// Parse a `0x` hex address string into `[u8;20]`.
pub(crate) fn evm_hex_to_bytes20(s: &str) -> Result<[u8; 20], AddrConvError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() != 40 {
        return Err(AddrConvError::InvalidHexLength);
    }

    let mut out = [0u8; 20];
    let chars: alloc::vec::Vec<char> = s.chars().collect();
    for i in 0..20 {
        let hi = hex_val(chars[2 * i])?;
        let lo = hex_val(chars[2 * i + 1])?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_val(c: char) -> Result<u8, AddrConvError> {
    match c {
        '0'..='9' => Ok((c as u8) - b'0'),
        'a'..='f' => Ok((c as u8) - b'a' + 10),
        'A'..='F' => Ok((c as u8) - b'A' + 10),
        _ => Err(AddrConvError::InvalidHexChar(c)),
    }
}

/// Convert `[u8; 20]` -> `[u64; 5]` by extracting the last 16 bytes.
/// Requires the first 4 bytes be zero.
pub fn bytes20_to_u64x5(bytes: [u8; 20]) -> Result<[u64; 5], AddrConvError> {
    if bytes[0..4] != [0, 0, 0, 0] {
        return Err(AddrConvError::NonZeroBytePrefix);
    }

    let w0 = u64::from_be_bytes(bytes[4..12].try_into().unwrap());
    let w1 = u64::from_be_bytes(bytes[12..20].try_into().unwrap());

    Ok([w0, w1, 0, 0, 0])
}

// Helper functions used by EthAddress
pub(crate) fn ethereum_address_to_account_id(
    address: &[u8; 20],
) -> Result<AccountId, AddrConvError> {
    let u64x5 = bytes20_to_u64x5(*address)?;
    let felts = [Felt::new(u64x5[0]), Felt::new(u64x5[1])];

    match AccountId::try_from(felts) {
        Ok(account_id) => Ok(account_id),
        Err(_) => Err(AddrConvError::NonZeroBytePrefix),
    }
}

pub(crate) fn account_id_to_ethereum_address(
    account_id: AccountId,
) -> Result<[u8; 20], AddrConvError> {
    let felts: [Felt; 2] = account_id.into();
    let u64x5 = [felts[0].as_int(), felts[1].as_int(), 0, 0, 0];
    u64x5_to_bytes20(u64x5)
}

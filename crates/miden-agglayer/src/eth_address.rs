use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use miden_core::FieldElement;
use miden_protocol::Felt;
use miden_protocol::account::AccountId;

use crate::utils::{
    AddrConvError,
    account_id_to_ethereum_address,
    bytes20_to_evm_hex,
    ethereum_address_to_account_id,
    evm_hex_to_bytes20,
};

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
        let mut result = Vec::with_capacity(5);
        for i in 0..5 {
            let start = i * 4;
            let chunk = &self.0[start..start + 4];
            let value = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            result.push(Felt::new(value as u64));
        }
        result
    }

    /// Converts the Ethereum address into an array of 5 [`Felt`] values.
    ///
    /// Each felt represents 4 bytes of the address in big-endian format.
    pub fn to_felt_array(&self) -> [Felt; 5] {
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

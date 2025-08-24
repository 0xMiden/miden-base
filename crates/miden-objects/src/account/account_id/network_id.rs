use alloc::string::{String, ToString};
use core::str::FromStr;

use bech32::Hrp;

use crate::errors::NetworkIdError;

/// A wrapper around HRP for custom network identifiers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CustomHrp {
    hrp_string: String,
}

impl CustomHrp {
    /// Creates a new `CustomHrp` from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid HRP according to bech32 rules
    pub fn new(s: &str) -> Result<Self, NetworkIdError> {
        Hrp::parse(s)
            .map(|_| CustomHrp { hrp_string: s.to_string() })
            .map_err(|source| NetworkIdError::NetworkIdParseError(source.to_string().into()))
    }

    /// Returns the string representation of this custom HRP.
    pub fn as_str(&self) -> &str {
        &self.hrp_string
    }

    /// Converts this `CustomHrp` to a `bech32::Hrp`.
    pub(crate) fn to_bech32_hrp(&self) -> Hrp {
        Hrp::parse(&self.hrp_string).expect("CustomHrp should always contain valid HRP")
    }

    /// Creates a `CustomHrp` from a `bech32::Hrp`.
    pub(crate) fn from_bech32_hrp(hrp: Hrp) -> Self {
        CustomHrp { hrp_string: hrp.to_string() }
    }
}

impl core::fmt::Display for CustomHrp {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.hrp_string)
    }
}

// This is essentially a wrapper around [`bech32::Hrp`] but that type does not actually appear in
// the public API since that crate does not have a stable release.

/// The identifier of a Miden network.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NetworkId {
    Mainnet,
    Testnet,
    Devnet,
    Custom(CustomHrp),
}

impl NetworkId {
    const MAINNET: &str = "mm";
    const TESTNET: &str = "mtst";
    const DEVNET: &str = "mdev";

    /// Constructs a new [`NetworkId`] from a string.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the string does not contain between 1 to 83 US-ASCII characters.
    /// - each character is not in the range 33-126.
    pub fn new(string: &str) -> Result<Self, NetworkIdError> {
        Hrp::parse(string)
            .map(Self::from_hrp)
            .map_err(|source| NetworkIdError::NetworkIdParseError(source.to_string().into()))
    }

    /// Constructs a new [`NetworkId`] from an [`Hrp`].
    ///
    /// This method should not be made public to avoid having `bech32` types in the public API.
    pub(crate) fn from_hrp(hrp: Hrp) -> Self {
        match hrp.as_str() {
            NetworkId::MAINNET => NetworkId::Mainnet,
            NetworkId::TESTNET => NetworkId::Testnet,
            NetworkId::DEVNET => NetworkId::Devnet,
            _ => NetworkId::Custom(CustomHrp::from_bech32_hrp(hrp)),
        }
    }

    /// Returns the [`Hrp`] of this network ID.
    ///
    /// This method should not be made public to avoid having `bech32` types in the public API.
    pub(crate) fn into_hrp(self) -> Hrp {
        match self {
            NetworkId::Mainnet => {
                Hrp::parse(NetworkId::MAINNET).expect("mainnet hrp should be valid")
            },
            NetworkId::Testnet => {
                Hrp::parse(NetworkId::TESTNET).expect("testnet hrp should be valid")
            },
            NetworkId::Devnet => Hrp::parse(NetworkId::DEVNET).expect("devnet hrp should be valid"),
            NetworkId::Custom(custom) => custom.to_bech32_hrp(),
        }
    }

    /// Returns the string representation of the network ID.
    pub fn as_str(&self) -> &str {
        match self {
            NetworkId::Mainnet => NetworkId::MAINNET,
            NetworkId::Testnet => NetworkId::TESTNET,
            NetworkId::Devnet => NetworkId::DEVNET,
            NetworkId::Custom(custom) => custom.as_str(),
        }
    }

    /// Returns `true` if the network ID is the Miden mainnet, `false` otherwise.
    pub fn is_mainnet(&self) -> bool {
        matches!(self, NetworkId::Mainnet)
    }

    /// Returns `true` if the network ID is the Miden testnet, `false` otherwise.
    pub fn is_testnet(&self) -> bool {
        matches!(self, NetworkId::Testnet)
    }

    /// Returns `true` if the network ID is the Miden devnet, `false` otherwise.
    pub fn is_devnet(&self) -> bool {
        matches!(self, NetworkId::Devnet)
    }
}

impl FromStr for NetworkId {
    type Err = NetworkIdError;

    /// Constructs a new [`NetworkId`] from a string.
    ///
    /// See [`NetworkId::new`] for details on errors.
    fn from_str(string: &str) -> Result<Self, Self::Err> {
        Self::new(string)
    }
}

impl core::fmt::Display for NetworkId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

//! Internal NetworkId type for account_id module.
//!
//! This is a minimal implementation that matches the NetworkId from miden-standards
//! but is defined here to avoid circular dependencies.

use bech32::Hrp;
use alloc::string::ToString;

/// The identifier of a Miden network.
///
/// This is a minimal version used within account_id module to avoid circular dependencies.
/// For the full implementation, see `miden_standards::address::NetworkId`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NetworkId {
    Mainnet,
    Testnet,
    Devnet,
    Custom(alloc::boxed::Box<CustomNetworkId>),
}

impl NetworkId {
    const MAINNET: &str = "mm";
    const TESTNET: &str = "mtst";
    const DEVNET: &str = "mdev";

    /// Constructs a new [`NetworkId`] from an [`Hrp`].
    pub(crate) fn from_hrp(hrp: Hrp) -> Self {
        match hrp.as_str() {
            Self::MAINNET => NetworkId::Mainnet,
            Self::TESTNET => NetworkId::Testnet,
            Self::DEVNET => NetworkId::Devnet,
            _ => NetworkId::Custom(alloc::boxed::Box::new(CustomNetworkId::from_hrp(hrp))),
        }
    }

    /// Returns the [`Hrp`] of this network ID.
    pub fn into_hrp(self) -> Hrp {
        match self {
            NetworkId::Mainnet => {
                Hrp::parse(Self::MAINNET).expect("mainnet hrp should be valid")
            },
            NetworkId::Testnet => {
                Hrp::parse(Self::TESTNET).expect("testnet hrp should be valid")
            },
            NetworkId::Devnet => Hrp::parse(Self::DEVNET).expect("devnet hrp should be valid"),
            NetworkId::Custom(custom) => custom.as_hrp(),
        }
    }
}

/// A wrapper around bech32 HRP for custom network identifiers.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CustomNetworkId {
    hrp: Hrp,
}

impl CustomNetworkId {
    /// Creates a new [`CustomNetworkId`] from a [`bech32::Hrp`].
    pub(crate) fn from_hrp(hrp: Hrp) -> Self {
        CustomNetworkId { hrp }
    }

    /// Converts this [`CustomNetworkId`] to a [`bech32::Hrp`].
    pub(crate) fn as_hrp(&self) -> Hrp {
        self.hrp
    }
}

#[cfg(any(test, feature = "testing"))]
impl alloc::str::FromStr for CustomNetworkId {
    type Err = crate::errors::NetworkIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Hrp::parse(s)
            .map(Self::from_hrp)
            .map_err(|source| crate::errors::NetworkIdError::NetworkIdParseError(source.to_string().into()))
    }
}


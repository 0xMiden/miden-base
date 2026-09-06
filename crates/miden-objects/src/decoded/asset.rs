//! Domain construction for decoded asset messages.
pub use proto::asset::DecodedAssetClass as AssetClass;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for AssetClass {
    type Verified = miden_protocol::asset::AssetClass;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.suffix, self.prefix))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::asset::AssetClass> for miden_protocol::asset::AssetClass {
    type Error = ConversionError;
    fn try_from(value: proto::asset::AssetClass) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::asset::DecodedAssetId as AssetId;

impl Verify for AssetId {
    type Verified = miden_protocol::asset::AssetId;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        match self.version {
            proto::asset::AssetVersion::V1 => {},
            proto::asset::AssetVersion::Unspecified => {
                return Err(VerificationError::UnspecifiedVersion);
            },
        }
        let composition = match self.composition {
            proto::asset::AssetComposition::None => miden_protocol::asset::AssetComposition::None,
            proto::asset::AssetComposition::Fungible => {
                miden_protocol::asset::AssetComposition::Fungible
            },
            proto::asset::AssetComposition::Custom => {
                miden_protocol::asset::AssetComposition::Custom
            },
            proto::asset::AssetComposition::Unspecified => {
                return Err(VerificationError::UnspecifiedComposition);
            },
        };
        Ok(Self::Verified::new(
            self.asset_class.verify().expect("infallible asset class"),
            self.faucet_id.verify()?,
            composition,
        )?)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("{0}")]
    AccountId(#[from] miden_protocol::errors::AccountIdError),
    #[error("asset id version is unspecified")]
    UnspecifiedVersion,
    #[error("asset composition is unspecified")]
    UnspecifiedComposition,
    #[error("invalid asset: {0}")]
    Asset(#[from] miden_protocol::errors::AssetError),
}
// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::asset::AssetId> for miden_protocol::asset::AssetId {
    type Error = ConversionError;
    fn try_from(value: proto::asset::AssetId) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::asset::DecodedAsset as Asset;

impl Verify for Asset {
    type Verified = miden_protocol::asset::Asset;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.asset_id.verify()?, self.value)?)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::asset::Asset> for miden_protocol::asset::Asset {
    type Error = ConversionError;
    fn try_from(value: proto::asset::Asset) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

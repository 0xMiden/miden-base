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

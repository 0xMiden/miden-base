//! Domain construction for decoded blockchain messages.
pub use proto::blockchain::DecodedTrackedMmrLeaf as TrackedMmrLeaf;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for TrackedMmrLeaf {
    type Verified = (u64, miden_protocol::Word, alloc::vec::Vec<miden_protocol::Word>);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.position, self.leaf, self.path))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::TrackedMmrLeaf>
    for (u64, miden_protocol::Word, alloc::vec::Vec<miden_protocol::Word>)
{
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::TrackedMmrLeaf) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::blockchain::DecodedBlockNumber as BlockNumber;

impl Verify for BlockNumber {
    type Verified = miden_protocol::block::BlockNumber;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(self.block_num.into())
    }
}

pub use proto::blockchain::DecodedFeeParameters as FeeParameters;

impl Verify for FeeParameters {
    type Verified = miden_protocol::block::FeeParameters;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.verification_base_fee))
    }
}

pub use proto::blockchain::DecodedNextProtocolConfig as NextProtocolConfig;

impl Verify for NextProtocolConfig {
    type Verified = miden_protocol::protocol_config::NextProtocolConfig;
    type Error = miden_protocol::errors::ProtocolConfigError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let effective_from = self.effective_from.verify().expect("infallible block number");
        Self::Verified::new(effective_from, self.protocol_config)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::NextProtocolConfig>
    for miden_protocol::protocol_config::NextProtocolConfig
{
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::NextProtocolConfig) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

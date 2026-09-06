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

pub use proto::blockchain::DecodedValidatorConfig as ValidatorConfig;

impl Verify for ValidatorConfig {
    type Verified = miden_protocol::block::ValidatorConfig;
    type Error = ValidatorConfigError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let keys = self
            .keys
            .into_iter()
            .map(Verify::verify)
            .collect::<Result<_, _>>()
            .expect("canonical public keys");
        Ok(Self::Verified::new(keys, self.quorum.try_into()?)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ValidatorConfigError {
    #[error("quorum is out of range: {0}")]
    Quorum(#[from] core::num::TryFromIntError),
    #[error("{0}")]
    Config(#[from] miden_protocol::errors::ValidatorConfigError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::blockchain::ValidatorConfig> for miden_protocol::block::ValidatorConfig {
    type Error = ConversionError;
    fn try_from(value: proto::blockchain::ValidatorConfig) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

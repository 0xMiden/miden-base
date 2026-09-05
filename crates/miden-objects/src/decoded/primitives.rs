//! Domain construction for decoded primitives messages.
pub use proto::primitives::DecodedMerklePath as MerklePath;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for MerklePath {
    type Verified = miden_protocol::crypto::merkle::MerklePath;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.siblings))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::MerklePath> for miden_protocol::crypto::merkle::MerklePath {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::MerklePath) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedSparseMerklePath as SparseMerklePath;

impl Verify for SparseMerklePath {
    type Verified = miden_protocol::crypto::merkle::SparseMerklePath;
    type Error = miden_protocol::crypto::merkle::MerkleError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Self::Verified::from_parts(self.empty_nodes_mask, self.siblings)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::SparseMerklePath>
    for miden_protocol::crypto::merkle::SparseMerklePath
{
    type Error = ConversionError;
    fn try_from(value: proto::primitives::SparseMerklePath) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

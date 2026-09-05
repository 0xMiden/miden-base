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

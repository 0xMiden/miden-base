//! Domain construction for decoded account messages.
pub use proto::account::DecodedStorageSlotId as StorageSlotId;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for StorageSlotId {
    type Verified = miden_protocol::account::StorageSlotId;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.suffix, self.prefix))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::account::StorageSlotId> for miden_protocol::account::StorageSlotId {
    type Error = ConversionError;
    fn try_from(value: proto::account::StorageSlotId) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

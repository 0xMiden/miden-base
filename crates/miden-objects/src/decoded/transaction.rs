//! Domain construction for decoded transaction messages.
pub use proto::transaction::DecodedTransactionId as TransactionId;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for TransactionId {
    type Verified = miden_protocol::transaction::TransactionId;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::from_raw(self.id))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::transaction::TransactionId> for miden_protocol::transaction::TransactionId {
    type Error = ConversionError;
    fn try_from(value: proto::transaction::TransactionId) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

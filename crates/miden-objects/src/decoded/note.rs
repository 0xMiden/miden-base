//! Domain construction for decoded note messages.
pub use proto::note::DecodedNoteId as NoteId;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for NoteId {
    type Verified = miden_protocol::note::NoteId;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::from_raw(self.id))
    }
}

impl TryFrom<proto::note::NoteId> for miden_protocol::Word {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteId) -> Result<Self, Self::Error> {
        value.decode_fields().map(|decoded| decoded.id)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteId> for miden_protocol::note::NoteId {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteId) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

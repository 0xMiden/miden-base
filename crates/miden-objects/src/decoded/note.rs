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

pub use proto::note::DecodedNoteStorage as NoteStorage;

impl Verify for NoteStorage {
    type Verified = miden_protocol::note::NoteStorage;
    type Error = miden_protocol::errors::NoteError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Self::Verified::new(self.items)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteStorage> for miden_protocol::note::NoteStorage {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteStorage) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::note::DecodedNoteAttachment as NoteAttachment;

impl Verify for NoteAttachment {
    type Verified = miden_protocol::note::NoteAttachment;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let scheme = miden_protocol::note::NoteAttachmentScheme::new(self.scheme.try_into()?)?;
        Ok(Self::Verified::with_words(scheme, self.words)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("{0}")]
    Note(#[from] miden_protocol::errors::NoteError),
    #[error("numeric value is out of range: {0}")]
    Number(#[from] core::num::TryFromIntError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteAttachment> for miden_protocol::note::NoteAttachment {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteAttachment) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::note::DecodedNoteAttachments as NoteAttachments;

impl Verify for NoteAttachments {
    type Verified = miden_protocol::note::NoteAttachments;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let attachments =
            self.attachments.into_iter().map(Verify::verify).collect::<Result<_, _>>()?;
        Ok(Self::Verified::new(attachments)?)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteAttachments> for miden_protocol::note::NoteAttachments {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteAttachments) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

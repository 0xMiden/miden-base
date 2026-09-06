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
    #[error("invalid note asset: {0}")]
    Asset(#[from] super::asset::VerificationError),
    #[error("note metadata version is unspecified")]
    UnspecifiedVersion,
    #[error("note type is unspecified")]
    UnspecifiedNoteType,
    #[error("too many attachment schemes")]
    TooManyAttachmentSchemes,
    #[error("invalid inclusion path: {0}")]
    Path(#[from] miden_protocol::crypto::merkle::MerkleError),
    #[error("invalid script entrypoint: {0}")]
    Entrypoint(#[from] miden_protocol::utils::serde::DeserializationError),
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

pub use proto::note::DecodedNoteScript as NoteScript;

impl Verify for NoteScript {
    type Verified = miden_protocol::note::NoteScript;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let entrypoint = miden_protocol::MastNodeId::from_u32_safe(self.entrypoint, &self.mast)?;
        Ok(Self::Verified::from_parts(alloc::sync::Arc::new(self.mast), entrypoint)?)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteScript> for miden_protocol::note::NoteScript {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteScript) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::note::DecodedNoteRecipient as NoteRecipient;

impl Verify for NoteRecipient {
    type Verified = miden_protocol::note::NoteRecipient;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.serial_num,
            self.script.verify()?,
            self.storage.verify()?,
        ))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteRecipient> for miden_protocol::note::NoteRecipient {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteRecipient) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::note::DecodedNoteInclusionProof as NoteInclusionProof;

impl Verify for NoteInclusionProof {
    type Verified = (miden_protocol::note::NoteId, miden_protocol::note::NoteInclusionProof);
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((
            self.note_id.verify().expect("infallible note ID"),
            miden_protocol::note::NoteInclusionProof::new(
                self.block_num.verify().expect("infallible block number"),
                self.note_index_in_block.try_into()?,
                self.inclusion_path.verify()?,
            )?,
        ))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteInclusionProof>
    for (miden_protocol::note::NoteId, miden_protocol::note::NoteInclusionProof)
{
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteInclusionProof) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::note::DecodedNoteMetadata as NoteMetadata;

impl Verify for NoteMetadata {
    type Verified = miden_protocol::note::NoteMetadata;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use miden_protocol::note::{
            NoteAttachmentHeader,
            NoteAttachmentScheme,
            NoteAttachments,
            NoteTag,
            NoteType,
            PartialNoteMetadata,
        };
        match self.version {
            proto::note::NoteVersion::V1 => {},
            proto::note::NoteVersion::Unspecified => {
                return Err(VerificationError::UnspecifiedVersion);
            },
        }
        let note_type = match self.note_type {
            proto::note::NoteType::Private => NoteType::Private,
            proto::note::NoteType::Public => NoteType::Public,
            proto::note::NoteType::Unspecified => {
                return Err(VerificationError::UnspecifiedNoteType);
            },
        };
        let partial =
            PartialNoteMetadata::new(self.sender, note_type).with_tag(NoteTag::new(self.tag));
        if self.attachment_schemes.len() > NoteAttachments::MAX_COUNT {
            return Err(VerificationError::TooManyAttachmentSchemes);
        }
        let mut headers = [NoteAttachmentHeader::absent(); NoteAttachments::MAX_COUNT];
        for (header, raw) in headers.iter_mut().zip(self.attachment_schemes) {
            let scheme: u16 = raw.try_into()?;
            if scheme != 0 {
                *header = NoteAttachmentHeader::new(NoteAttachmentScheme::new(scheme)?);
            }
        }
        Ok(Self::Verified::from_parts(partial, headers, self.attachments_commitment))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteMetadata> for miden_protocol::note::NoteMetadata {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteMetadata) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::note::DecodedNoteDetails as NoteDetails;

impl Verify for NoteDetails {
    type Verified = miden_protocol::note::NoteDetails;
    type Error = VerificationError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let assets = self.assets.into_iter().map(Verify::verify).collect::<Result<_, _>>()?;
        let assets = miden_protocol::note::NoteAssets::new(assets)?;
        Ok(Self::Verified::new(assets, self.recipient.verify()?))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::note::NoteDetails> for miden_protocol::note::NoteDetails {
    type Error = ConversionError;
    fn try_from(value: proto::note::NoteDetails) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

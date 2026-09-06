use alloc::format;

use miden_protocol::Word;
use miden_protocol::note::{
    Note,
    NoteAttachment,
    NoteAttachments,
    NoteDetails,
    NoteHeader,
    NoteId,
    NoteInclusionProof,
    NoteMetadata,
    NoteRecipient,
    NoteScript,
    NoteStorage,
    NoteTag,
    NoteType,
    PartialNoteMetadata,
};

use super::{MessageDecodeExt, MessageDecoder, required};
use crate::{ConversionError, ConversionResultExt, proto};

// NOTE TYPE
// ================================================================================================

impl From<NoteType> for proto::note::NoteType {
    fn from(note_type: NoteType) -> Self {
        match note_type {
            NoteType::Private => proto::note::NoteType::Private,
            NoteType::Public => proto::note::NoteType::Public,
        }
    }
}

impl TryFrom<proto::note::NoteType> for NoteType {
    type Error = ConversionError;

    fn try_from(note_type: proto::note::NoteType) -> Result<Self, Self::Error> {
        match note_type {
            proto::note::NoteType::Private => Ok(NoteType::Private),
            proto::note::NoteType::Public => Ok(NoteType::Public),
            proto::note::NoteType::Unspecified => {
                Err(ConversionError::message("enum variant discriminant out of range"))
            },
        }
    }
}

// NOTE METADATA
// ================================================================================================

impl From<NoteMetadata> for proto::note::NoteMetadata {
    fn from(metadata: NoteMetadata) -> Self {
        Self {
            version: proto::note::NoteVersion::V1 as i32,
            sender: Some(metadata.sender().into()),
            note_type: proto::note::NoteType::from(metadata.note_type()) as i32,
            tag: metadata.tag().as_u32(),
            attachment_schemes: metadata
                .attachment_headers()
                .iter()
                .map(|header| u32::from(header.scheme().map_or(0, |scheme| scheme.as_u16())))
                .collect(),
            attachments_commitment: Some(metadata.attachments_commitment().into()),
        }
    }
}

impl From<PartialNoteMetadata> for proto::note::PartialNoteMetadata {
    fn from(metadata: PartialNoteMetadata) -> Self {
        Self {
            version: proto::note::NoteVersion::V1 as i32,
            sender: Some(metadata.sender().into()),
            note_type: proto::note::NoteType::from(metadata.note_type()) as i32,
            tag: metadata.tag().as_u32(),
        }
    }
}

impl TryFrom<proto::note::PartialNoteMetadata> for PartialNoteMetadata {
    type Error = ConversionError;

    fn try_from(metadata: proto::note::PartialNoteMetadata) -> Result<Self, Self::Error> {
        decode_note_version(metadata.version).context("version")?;
        decode_partial_note_metadata::<proto::note::PartialNoteMetadata>(
            metadata.sender,
            metadata.note_type,
            metadata.tag,
        )
    }
}
// NOTE ATTACHMENTS
// ================================================================================================

impl From<&NoteAttachment> for proto::note::NoteAttachment {
    fn from(attachment: &NoteAttachment) -> Self {
        Self {
            scheme: u32::from(attachment.attachment_scheme().as_u16()),
            words: attachment.content().as_words().iter().map(Into::into).collect(),
        }
    }
}

impl From<NoteAttachments> for proto::note::NoteAttachments {
    fn from(attachments: NoteAttachments) -> Self {
        Self::from(&attachments)
    }
}

impl From<&NoteAttachments> for proto::note::NoteAttachments {
    fn from(attachments: &NoteAttachments) -> Self {
        Self {
            attachments: attachments.iter().map(Into::into).collect(),
        }
    }
}

// NOTE DETAILS
// ================================================================================================

impl From<NoteStorage> for proto::note::NoteStorage {
    fn from(storage: NoteStorage) -> Self {
        Self::from(&storage)
    }
}

impl From<&NoteStorage> for proto::note::NoteStorage {
    fn from(storage: &NoteStorage) -> Self {
        Self {
            items: storage.items().iter().map(Into::into).collect(),
        }
    }
}

impl From<NoteRecipient> for proto::note::NoteRecipient {
    fn from(recipient: NoteRecipient) -> Self {
        Self::from(&recipient)
    }
}

impl From<&NoteRecipient> for proto::note::NoteRecipient {
    fn from(recipient: &NoteRecipient) -> Self {
        Self {
            serial_num: Some(recipient.serial_num().into()),
            script: Some(recipient.script().into()),
            storage: Some(recipient.storage().into()),
        }
    }
}

impl From<NoteDetails> for proto::note::NoteDetails {
    fn from(details: NoteDetails) -> Self {
        Self::from(&details)
    }
}

impl From<&NoteDetails> for proto::note::NoteDetails {
    fn from(details: &NoteDetails) -> Self {
        Self {
            assets: details.assets().iter().copied().map(Into::into).collect(),
            recipient: Some(details.recipient().into()),
        }
    }
}

// NOTE
// ================================================================================================

impl From<Note> for proto::note::Note {
    fn from(note: Note) -> Self {
        let (assets, metadata, recipient, attachments) = note.into_parts();
        Self {
            metadata: Some(metadata.into_partial_metadata().into()),
            note_details: Some(NoteDetails::new(assets, recipient).into()),
            note_attachments: Some(attachments.into()),
        }
    }
}

impl TryFrom<proto::note::Note> for Note {
    type Error = ConversionError;

    fn try_from(proto_note: proto::note::Note) -> Result<Self, Self::Error> {
        let decoder = proto_note.decoder();
        let proto::note::Note { metadata, note_details, note_attachments } = proto_note;

        let partial_metadata = required!(decoder, metadata)?;

        let note_details: NoteDetails = required!(decoder, note_details)?;
        let (assets, recipient) = note_details.into_parts();
        let attachments = decode_note_attachments::<proto::note::Note>(note_attachments)?;

        Ok(Note::with_attachments(assets, partial_metadata, recipient, attachments))
    }
}

// NOTE ID
// ================================================================================================

impl From<Word> for proto::note::NoteId {
    fn from(digest: Word) -> Self {
        Self { id: Some(digest.into()) }
    }
}

impl From<&NoteId> for proto::note::NoteId {
    fn from(note_id: &NoteId) -> Self {
        Self { id: Some(note_id.as_word().into()) }
    }
}

impl From<(&NoteId, &NoteInclusionProof)> for proto::note::NoteInclusionProof {
    fn from((note_id, proof): (&NoteId, &NoteInclusionProof)) -> Self {
        Self {
            note_id: Some(note_id.into()),
            block_num: Some(proof.location().block_num().into()),
            note_index_in_block: proof.location().block_note_tree_index().into(),
            inclusion_path: Some(proof.note_path().clone().into()),
        }
    }
}

impl TryFrom<&proto::note::NoteInclusionProof> for (NoteId, NoteInclusionProof) {
    type Error = ConversionError;
    fn try_from(value: &proto::note::NoteInclusionProof) -> Result<Self, Self::Error> {
        value.clone().try_into()
    }
}

// NOTE HEADER
// ================================================================================================

impl From<NoteHeader> for proto::note::NoteHeader {
    fn from(header: NoteHeader) -> Self {
        Self {
            details_commitment: Some(header.details_commitment().as_word().into()),
            metadata: Some(header.into_metadata().into()),
        }
    }
}

// NOTE SCRIPT
// ================================================================================================

impl From<NoteScript> for proto::note::NoteScript {
    fn from(script: NoteScript) -> Self {
        Self::from(&script)
    }
}

impl From<&NoteScript> for proto::note::NoteScript {
    fn from(script: &NoteScript) -> Self {
        Self {
            entrypoint: script.entrypoint().into(),
            mast: Some(script.mast().as_ref().into()),
        }
    }
}

// HELPERS
// ================================================================================================

fn decode_note_version(version: i32) -> Result<(), ConversionError> {
    match proto::note::NoteVersion::try_from(version) {
        Ok(proto::note::NoteVersion::V1) => Ok(()),
        Ok(proto::note::NoteVersion::Unspecified) => {
            Err(ConversionError::message("note metadata version is unspecified"))
        },
        Err(error) => Err(ConversionError::with_source(
            format!("unknown note metadata version {version}"),
            error,
        )),
    }
}

fn decode_partial_note_metadata<M: prost::Message>(
    sender: Option<proto::account::AccountId>,
    note_type: i32,
    tag: u32,
) -> Result<PartialNoteMetadata, ConversionError> {
    let decoder = MessageDecoder::<M>::default();
    let sender = required!(decoder, sender)?;
    let note_type = proto::note::NoteType::try_from(note_type)
        .map_err(|_| ConversionError::message("enum variant discriminant out of range"))?
        .try_into()
        .context("note_type")?;
    let tag = NoteTag::new(tag);
    Ok(PartialNoteMetadata::new(sender, note_type).with_tag(tag))
}

/// Requires and decodes the structured attachments carried by a note message.
fn decode_note_attachments<M: prost::Message>(
    note_attachments: Option<proto::note::NoteAttachments>,
) -> Result<NoteAttachments, ConversionError> {
    let decoder = MessageDecoder::<M>::default();
    required!(decoder, note_attachments)
}

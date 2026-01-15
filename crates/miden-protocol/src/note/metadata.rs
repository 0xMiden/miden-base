use super::{
    AccountId,
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Felt,
    NoteTag,
    NoteType,
    Serializable,
    Word,
};
use crate::note::{NoteAttachment, NoteAttachmentContentType, NoteAttachmentType};
use crate::{Hasher, NoteError};

// NOTE METADATA
// ================================================================================================

/// The metadata associated with a note.
///
/// Note metadata consists of two parts:
/// - The header of the metadata, which consists of:
///   - the sender of the note
///   - the [`NoteType`]
///   - the [`NoteTag`]
///   - type information about the [`NoteAttachment`].
/// - The optional [`NoteAttachment`].
///
/// # Word layout & validity
///
/// [`NoteMetadata`] can be encoded into two words, a header and an attachment word.
///
/// The header word has the following layout:
///
/// ```text
/// 0th felt: [sender_id_suffix (56 bits) | 6 zero bits | note_type (2 bit)]
/// 1st felt: [sender_id_prefix (64 bits)]
/// 2nd felt: [32 zero bits | note_tag (32 bits)]
/// 3rd felt: [30 zero bits | attachment_content_type (2 bits) | attachment_type (32 bits)]
/// ```
///
/// The felt validity of each part of the layout is guaranteed:
/// - 1st felt: The lower 8 bits of the account ID suffix are `0` by construction, so that they can
///   be overwritten with other data. The suffix' most significant bit must be zero such that the
///   entire felt retains its validity even if all of its lower 8 bits are be set to `1`. So the
///   note type can be comfortably encoded.
/// - 2nd felt: Is equivalent to the prefix of the account ID so it inherits its validity.
/// - 3rd felt: The upper 32 bits are always zero.
/// - 4th felt: The upper 30 bits are always zero.
///
/// The value of the attachment word depends on the
/// [`NoteAttachmentContentType`](crate::note::NoteAttachmentContentType):
/// - [`NoteAttachmentContentType::None`](crate::note::NoteAttachmentContentType::None): Empty word.
/// - [`NoteAttachmentContentType::Word`](crate::note::NoteAttachmentContentType::Word): The raw
///   word itself.
/// - [`NoteAttachmentContentType::Array`](crate::note::NoteAttachmentContentType::Array): The
///   commitment to the elements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteMetadata {
    /// The ID of the account which created the note.
    sender: AccountId,

    /// Defines how the note is to be stored (e.g. public or private).
    note_type: NoteType,

    /// A value which can be used by the recipient(s) to identify notes intended for them.
    tag: NoteTag,

    /// The optional attachment of a note's metadata.
    ///
    /// Defaults to [`NoteAttachment::default`].
    attachment: NoteAttachment,
}

impl NoteMetadata {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns a new [`NoteMetadata`] instantiated with the specified parameters.
    pub fn new(sender: AccountId, note_type: NoteType, tag: NoteTag) -> Self {
        Self {
            sender,
            note_type,
            tag,
            attachment: NoteAttachment::default(),
        }
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the account which created the note.
    pub fn sender(&self) -> AccountId {
        self.sender
    }

    /// Returns the note's type.
    pub fn note_type(&self) -> NoteType {
        self.note_type
    }

    /// Returns the tag associated with the note.
    pub fn tag(&self) -> NoteTag {
        self.tag
    }

    /// Returns the attachment of the note.
    pub fn attachment(&self) -> &NoteAttachment {
        &self.attachment
    }

    /// Returns `true` if the note is private.
    pub fn is_private(&self) -> bool {
        self.note_type == NoteType::Private
    }

    /// Returns the header of a [`NoteMetadata`] as a [`Word`].
    ///
    /// See [`NoteMetadata`] docs for more details.
    fn to_header(&self) -> NoteMetadataHeader {
        NoteMetadataHeader {
            sender: self.sender,
            note_type: self.note_type,
            tag: self.tag,
            attachment_content_type: self.attachment().content().content_type(),
            attachment_type: self.attachment.attachment_type(),
        }
    }

    /// Returns the [`Word`] that represents the header of a [`NoteMetadata`].
    ///
    /// See [`NoteMetadata`] docs for more details.
    pub fn to_header_word(&self) -> Word {
        Word::from(self.to_header())
    }

    /// Returns the [`Word`] that represents the attachment of a [`NoteMetadata`].
    ///
    /// See [`NoteMetadata`] docs for more details.
    pub fn to_attachment_word(&self) -> Word {
        self.attachment.content().to_word()
    }

    /// Returns the commitment to the note metadata, which is defined as:
    ///
    /// ```text
    /// hash(NOTE_METADATA_HEADER || NOTE_METADATA_ATTACHMENT)
    /// ```
    pub fn to_commitment(&self) -> Word {
        Hasher::merge(&[self.to_header_word(), self.to_attachment_word()])
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Overwrites the note's attachment with the provided one.
    pub fn set_attachment(&mut self, attachment: NoteAttachment) {
        self.attachment = attachment;
    }

    /// Overwrites the note's attachment with the provided one.
    pub fn with_attachment(mut self, attachment: NoteAttachment) -> Self {
        self.attachment = attachment;
        self
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for NoteMetadata {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.note_type().write_into(target);
        self.sender().write_into(target);
        self.tag().write_into(target);
        self.attachment().write_into(target);
    }
}

impl Deserializable for NoteMetadata {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let note_type = NoteType::read_from(source)?;
        let sender = AccountId::read_from(source)?;
        let tag = NoteTag::read_from(source)?;
        let attachment = NoteAttachment::read_from(source)?;

        Ok(NoteMetadata::new(sender, note_type, tag).with_attachment(attachment))
    }
}

// NOTE METADATA HEADER
// ================================================================================================

/// The header representation of [`NoteMetadata`].
///
/// See the metadata's type for details on this type's [`Word`] layout.
///
/// This is intended to be a private type meant for encapsulating the conversion from and to words.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NoteMetadataHeader {
    sender: AccountId,
    note_type: NoteType,
    tag: NoteTag,
    attachment_content_type: NoteAttachmentContentType,
    attachment_type: NoteAttachmentType,
}

impl From<NoteMetadataHeader> for Word {
    fn from(header: NoteMetadataHeader) -> Self {
        let mut metadata = Word::empty();

        metadata[0] = merge_sender_suffix_and_note_type(header.sender.suffix(), header.note_type);
        metadata[1] = header.sender.prefix().as_felt();
        metadata[2] = Felt::from(header.tag);
        metadata[3] =
            merge_attachment_type_info(header.attachment_content_type, header.attachment_type);

        metadata
    }
}

impl TryFrom<Word> for NoteMetadataHeader {
    type Error = NoteError;

    /// Decodes a [`NoteMetadataHeader`] from a [`Word`].
    fn try_from(word: Word) -> Result<Self, Self::Error> {
        let (sender_suffix, note_type) = unmerge_sender_suffix_and_note_type(word[0])?;
        let sender_prefix = word[1];
        let tag = u32::try_from(word[2]).map(NoteTag::new).map_err(|_| {
            NoteError::other("failed to convert note tag from metadata header to u32")
        })?;
        let (attachment_content_type, attachment_type) = unmerge_attachment_type_info(word[3])?;

        let sender = AccountId::try_from([sender_prefix, sender_suffix]).map_err(|source| {
            NoteError::other_with_source("failed to decode account ID from metadata header", source)
        })?;

        Ok(Self {
            sender,
            note_type,
            tag,
            attachment_content_type,
            attachment_type,
        })
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Merges the suffix of an [`AccountId`] and the [`NoteType`] into a single [`Felt`].
///
/// The layout is as follows:
///
/// ```text
/// [sender_id_suffix (56 bits) | 6 zero bits | note_type (2 bits)]
/// ```
///
/// The most significant bit of the suffix is guaranteed to be zero, so the felt retains its
/// validity.
///
/// The `sender_id_suffix` is the suffix of the sender's account ID.
fn merge_sender_suffix_and_note_type(sender_id_suffix: Felt, note_type: NoteType) -> Felt {
    let mut merged = sender_id_suffix.as_int();

    let note_type_byte = note_type as u8;
    debug_assert!(note_type_byte < 4, "note type must not contain values >= 4");
    merged |= note_type_byte as u64;

    // SAFETY: The most significant bit of the suffix is zero by construction so the u64 will be a
    // valid felt.
    Felt::try_from(merged).expect("encoded value should be a valid felt")
}

/// Unmerges the sender ID suffix and note type.
fn unmerge_sender_suffix_and_note_type(element: Felt) -> Result<(Felt, NoteType), NoteError> {
    const NOTE_TYPE_MASK: u8 = 0b11;
    // Inverts the note type mask.
    const SENDER_SUFFIX_MASK: u64 = !(NOTE_TYPE_MASK as u64);

    let note_type_byte = element.as_int() as u8 & NOTE_TYPE_MASK;
    let note_type = NoteType::try_from(note_type_byte).map_err(|source| {
        NoteError::other_with_source("failed to decode note type from metadata header", source)
    })?;

    // No bits were set so felt should still be valid.
    let sender_suffix =
        Felt::try_from(element.as_int() & SENDER_SUFFIX_MASK).expect("felt should still be valid");

    Ok((sender_suffix, note_type))
}

/// Merges the [`NoteAttachmentType`] and [`NoteAttachmentContentType`] into a single [`Felt`].
///
/// The layout is as follows:
///
/// ```text
/// [30 zero bits | attachment_content_type (2 bits) | attachment_type (32 bits)]
/// ```
fn merge_attachment_type_info(
    attachment_content_type: NoteAttachmentContentType,
    attachment_type: NoteAttachmentType,
) -> Felt {
    debug_assert!(
        attachment_content_type.as_u8() < 4,
        "attachment content type should fit into two bits"
    );
    let mut merged = (attachment_content_type.as_u8() as u64) << 32;
    let attachment_type = attachment_type.as_u32();
    merged |= attachment_type as u64;

    Felt::try_from(merged).expect("the upper bit should be zero and the felt therefore valid")
}

/// Unmerges the attachment content type and attachment type.
fn unmerge_attachment_type_info(
    element: Felt,
) -> Result<(NoteAttachmentContentType, NoteAttachmentType), NoteError> {
    let attachment_type = element.as_int() as u32;
    let attachment_content_type = (element.as_int() >> 32) as u8;

    let attachment_type = NoteAttachmentType::new(attachment_type);
    let attachment_content_type = NoteAttachmentContentType::try_from(attachment_content_type)
        .map_err(|source| {
            NoteError::other_with_source(
                "failed to decode attachment content type from metadata header",
                source,
            )
        })?;

    Ok((attachment_content_type, attachment_type))
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {

    use super::*;
    use crate::note::NoteAttachmentType;
    use crate::testing::account_id::ACCOUNT_ID_MAX_ONES;

    #[rstest::rstest]
    #[case::attachment_none(NoteAttachment::default())]
    #[case::attachment_raw(NoteAttachment::new_word(NoteAttachmentType::new(0), Word::from([3, 4, 5, 6u32])))]
    #[case::attachment_commitment(NoteAttachment::new_array(
        NoteAttachmentType::new(u32::MAX),
        vec![Felt::new(5), Felt::new(6), Felt::new(7)],
    )?)]
    #[test]
    fn note_metadata_serde(#[case] attachment: NoteAttachment) -> anyhow::Result<()> {
        // Use the Account ID with the maximum one bits to test if the merge function always
        // produces valid felts.
        let sender = AccountId::try_from(ACCOUNT_ID_MAX_ONES).unwrap();
        let note_type = NoteType::Public;
        let tag = NoteTag::new(u32::MAX);
        let metadata = NoteMetadata::new(sender, note_type, tag).with_attachment(attachment);

        // Serialization Roundtrip
        let deserialized = NoteMetadata::read_from_bytes(&metadata.to_bytes())?;
        assert_eq!(deserialized, metadata);

        // Metadata Header Roundtrip
        let header = NoteMetadataHeader::try_from(metadata.to_header_word())?;
        assert_eq!(header, metadata.to_header());

        Ok(())
    }
}

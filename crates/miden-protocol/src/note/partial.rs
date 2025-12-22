use super::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    NoteAssets,
    NoteHeader,
    NoteId,
    NoteMetadata,
    Serializable,
};
use crate::Word;

// PARTIAL NOTE
// ================================================================================================

/// Partial information about a note.
///
/// Partial note consists of [NoteMetadata], [NoteAssets], and a recipient digest (see
/// [super::NoteRecipient]). However, it does not contain detailed recipient info, including
/// note script, note inputs, and note's serial number. This means that a partial note is
/// sufficient to compute note ID and note header, but not sufficient to compute note nullifier,
/// and generally does not have enough info to execute the note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialNote {
    metadata: NoteMetadata,
    recipient_digest: Word,
    assets: NoteAssets,
}

impl PartialNote {
    /// Returns a new [PartialNote] instantiated from the provided parameters.
    pub fn new(metadata: NoteMetadata, recipient_digest: Word, assets: NoteAssets) -> Self {
        Self { metadata, recipient_digest, assets }
    }

    /// Returns the ID corresponding to this note.
    pub fn id(&self) -> NoteId {
        NoteId::new(self.recipient_digest, self.assets.commitment())
    }

    /// Returns the metadata associated with this note.
    pub fn metadata(&self) -> &NoteMetadata {
        &self.metadata
    }

    /// Returns the digest of the recipient associated with this note.
    ///
    /// See [super::NoteRecipient] for more info.
    pub fn recipient_digest(&self) -> Word {
        self.recipient_digest
    }

    /// Returns a list of assets associated with this note.
    pub fn assets(&self) -> &NoteAssets {
        &self.assets
    }
}

impl From<&PartialNote> for NoteHeader {
    fn from(note: &PartialNote) -> Self {
        NoteHeader::new(note.id(), note.metadata)
    }
}

impl From<PartialNote> for NoteHeader {
    fn from(note: PartialNote) -> Self {
        NoteHeader::new(note.id(), note.metadata)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for PartialNote {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.metadata.write_into(target);
        self.recipient_digest.write_into(target);
        self.assets.write_into(target)
    }
}

impl Deserializable for PartialNote {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let metadata = NoteMetadata::read_from(source)?;
        let recipient_digest = Word::read_from(source)?;
        let assets = NoteAssets::read_from(source)?;

        Ok(Self::new(metadata, recipient_digest, assets))
    }
}

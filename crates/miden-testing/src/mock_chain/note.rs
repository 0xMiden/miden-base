use miden_objects::{
    NoteError,
    note::{Note, NoteId, NoteInclusionProof, NoteMetadata, NoteType},
    transaction::InputNote,
};

// MOCK CHAIN NOTE
// ================================================================================================

/// Represents a note that is stored in the mock chain.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum MockChainNote {
    /// Details for a private note only include its [`NoteMetadata`] and [`NoteInclusionProof`].
    /// Other details needed to consume the note are expected to be stored locally, off-chain.
    Private(NoteId, NoteMetadata, NoteInclusionProof),
    /// Contains the full [`Note`] object alongside its [`NoteInclusionProof`].
    Public(Note, NoteInclusionProof),
}

impl MockChainNote {
    /// Returns the note's inclusion details.
    pub fn inclusion_proof(&self) -> &NoteInclusionProof {
        match self {
            MockChainNote::Private(_, _, inclusion_proof)
            | MockChainNote::Public(_, inclusion_proof) => inclusion_proof,
        }
    }

    /// Returns the note's metadata.
    pub fn metadata(&self) -> &NoteMetadata {
        match self {
            MockChainNote::Private(_, metadata, _) => metadata,
            MockChainNote::Public(note, _) => note.metadata(),
        }
    }

    /// Returns the note's ID.
    pub fn id(&self) -> NoteId {
        match self {
            MockChainNote::Private(id, ..) => *id,
            MockChainNote::Public(note, _) => note.id(),
        }
    }

    /// Returns the underlying note if it is public.
    pub fn note(&self) -> Option<&Note> {
        match self {
            MockChainNote::Private(..) => None,
            MockChainNote::Public(note, _) => Some(note),
        }
    }
}

impl TryFrom<MockChainNote> for InputNote {
    type Error = NoteError;

    fn try_from(value: MockChainNote) -> Result<Self, Self::Error> {
        match value {
            MockChainNote::Private(..) => Err(NoteError::PublicNoteRequired(NoteType::Private)),
            MockChainNote::Public(note, proof) => Ok(InputNote::Authenticated { note, proof }),
        }
    }
}

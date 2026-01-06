use alloc::collections::BTreeSet;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt::Debug;

use crate::account::AccountHeader;
use crate::asset::FungibleAsset;
use crate::block::BlockNumber;
use crate::note::{
    Note,
    NoteAssets,
    NoteHeader,
    NoteId,
    NoteMetadata,
    NoteRecipient,
    PartialNote,
    compute_note_commitment,
};
use crate::utils::serde::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use crate::{
    Felt,
    Hasher,
    MAX_OUTPUT_NOTES_PER_TX,
    NOTE_MAX_SIZE,
    PublicOutputNoteError,
    TransactionOutputError,
    Word,
};

// TRANSACTION OUTPUTS
// ================================================================================================

/// Describes the result of executing a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionOutputs {
    /// Information related to the account's final state.
    pub account: AccountHeader,
    /// The commitment to the delta computed by the transaction kernel.
    pub account_delta_commitment: Word,
    /// Set of output notes created by the transaction.
    pub output_notes: RawOutputNotes,
    /// The fee of the transaction.
    pub fee: FungibleAsset,
    /// Defines up to which block the transaction is considered valid.
    pub expiration_block_num: BlockNumber,
}

impl TransactionOutputs {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// The index of the word at which the final account nonce is stored on the output stack.
    pub const OUTPUT_NOTES_COMMITMENT_WORD_IDX: usize = 0;

    /// The index of the word at which the account update commitment is stored on the output stack.
    pub const ACCOUNT_UPDATE_COMMITMENT_WORD_IDX: usize = 1;

    /// The index of the word at which the fee asset is stored on the output stack.
    pub const FEE_ASSET_WORD_IDX: usize = 2;

    /// The index of the item at which the expiration block height is stored on the output stack.
    pub const EXPIRATION_BLOCK_ELEMENT_IDX: usize = 12;
}

impl Serializable for TransactionOutputs {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.account.write_into(target);
        self.account_delta_commitment.write_into(target);
        self.output_notes.write_into(target);
        self.fee.write_into(target);
        self.expiration_block_num.write_into(target);
    }
}

impl Deserializable for TransactionOutputs {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let account = AccountHeader::read_from(source)?;
        let account_delta_commitment = Word::read_from(source)?;
        let output_notes = RawOutputNotes::read_from(source)?;
        let fee = FungibleAsset::read_from(source)?;
        let expiration_block_num = BlockNumber::read_from(source)?;

        Ok(Self {
            account,
            account_delta_commitment,
            output_notes,
            fee,
            expiration_block_num,
        })
    }
}

// OUTPUT NOTES
// ================================================================================================

/// Contains a list of output notes of a transaction. The list can be empty if the transaction does
/// not produce any notes.
///
/// This struct is generic over the note type `N`, allowing it to be used with both
/// [`RawOutputNote`] (in [`ExecutedTransaction`](super::ExecutedTransaction)) and
/// [`ProvenOutputNote`] (in [`ProvenTransaction`](super::ProvenTransaction)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputNotes<N> {
    notes: Vec<N>,
    commitment: Word,
}

pub type RawOutputNotes = OutputNotes<RawOutputNote>;

pub type ProvenOutputNotes = OutputNotes<ProvenOutputNote>;

impl<N> OutputNotes<N>
where
    for<'a> NoteHeader: From<&'a N>,
    for<'a> NoteId: From<&'a N>,
{
    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the commitment to the output notes.
    ///
    /// The commitment is computed as a sequential hash of (hash, metadata) tuples for the notes
    /// created in a transaction.
    pub fn commitment(&self) -> Word {
        self.commitment
    }

    /// Returns total number of output notes.
    pub fn num_notes(&self) -> usize {
        self.notes.len()
    }

    /// Returns true if this [OutputNotes] does not contain any notes.
    pub fn is_empty(&self) -> bool {
        self.notes.is_empty()
    }

    /// Returns a reference to the note located at the specified index.
    pub fn get_note(&self, idx: usize) -> &N {
        &self.notes[idx]
    }

    // ITERATORS
    // --------------------------------------------------------------------------------------------

    /// Returns an iterator over notes in this [OutputNotes].
    pub fn iter(&self) -> impl Iterator<Item = &N> {
        self.notes.iter()
    }

    // HELPERS
    // --------------------------------------------------------------------------------------------

    /// Computes a commitment to output notes.
    ///
    /// For a non-empty list of notes, this is a sequential hash of (note_id, metadata) tuples for
    /// the notes created in a transaction. For an empty list, [EMPTY_WORD] is returned.
    pub(crate) fn compute_commitment(notes: impl ExactSizeIterator<Item = NoteHeader>) -> Word {
        if notes.len() == 0 {
            return Word::empty();
        }

        let mut elements: Vec<Felt> = Vec::with_capacity(notes.len() * 8);
        for note_header in notes {
            elements.extend_from_slice(note_header.id().as_elements());
            elements.extend_from_slice(Word::from(note_header.metadata()).as_elements());
        }

        Hasher::hash_elements(&elements)
    }
}

impl RawOutputNotes {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// Returns new [RawOutputNotes] instantiated from the provided vector of notes.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The total number of notes is greater than [`MAX_OUTPUT_NOTES_PER_TX`].
    /// - The vector of notes contains duplicates.
    pub fn new(notes: Vec<RawOutputNote>) -> Result<Self, TransactionOutputError> {
        if notes.len() > MAX_OUTPUT_NOTES_PER_TX {
            return Err(TransactionOutputError::TooManyOutputNotes(notes.len()));
        }

        let mut seen_notes = BTreeSet::new();
        for note in notes.iter() {
            let note_id = note.id();
            if !seen_notes.insert(note_id) {
                return Err(TransactionOutputError::DuplicateOutputNote(note_id));
            }
        }

        let commitment = Self::compute_commitment(notes.iter().map(NoteHeader::from));

        Ok(Self { notes, commitment })
    }
}

impl ProvenOutputNotes {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// Returns new [ProvenOutputNotes] instantiated from the provided vector of notes.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The total number of notes is greater than [`MAX_OUTPUT_NOTES_PER_TX`].
    /// - The vector of notes contains duplicates.
    pub fn new(notes: Vec<ProvenOutputNote>) -> Result<Self, TransactionOutputError> {
        if notes.len() > MAX_OUTPUT_NOTES_PER_TX {
            return Err(TransactionOutputError::TooManyOutputNotes(notes.len()));
        }

        let mut seen_notes = BTreeSet::new();
        for note in notes.iter() {
            let note_id = NoteId::from(note);
            if !seen_notes.insert(note_id) {
                return Err(TransactionOutputError::DuplicateOutputNote(note_id));
            }
        }

        let commitment = Self::compute_commitment(notes.iter().map(NoteHeader::from));

        Ok(Self { notes, commitment })
    }
}

// SERIALIZATION
// ------------------------------------------------------------------------------------------------

impl Serializable for RawOutputNotes {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        // assert is OK here because we enforce max number of notes in the constructor
        assert!(self.notes.len() <= u16::MAX.into());
        target.write_u16(self.notes.len() as u16);
        target.write_many(&self.notes);
    }
}

impl Deserializable for RawOutputNotes {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let num_notes = source.read_u16()?;
        let notes = source.read_many::<RawOutputNote>(num_notes.into())?;
        Self::new(notes).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

impl Serializable for ProvenOutputNotes {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        // assert is OK here because we enforce max number of notes in the constructor
        assert!(self.notes.len() <= u16::MAX.into());
        target.write_u16(self.notes.len() as u16);
        target.write_many(&self.notes);
    }
}

impl Deserializable for ProvenOutputNotes {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let num_notes = source.read_u16()?;
        let notes = source.read_many::<ProvenOutputNote>(num_notes.into())?;
        Self::new(notes).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// RAW OUTPUT NOTE
// ================================================================================================

const RAW_FULL: u8 = 0;
const RAW_PARTIAL: u8 = 1;
const RAW_HEADER: u8 = 2;

/// The types of note outputs produced during transaction execution (before proving).
///
/// This enum represents notes as they exist immediately after transaction execution,
/// before they are processed for inclusion in a proven transaction. It includes:
/// - Full notes with all details (public or private)
/// - Partial notes (notes created with only recipient digest, not full recipient details)
/// - Note headers (minimal note information)
///
/// During proving, these are converted to [`ProvenOutputNote`] via the [`shrink`](Self::shrink)
/// method, which enforces size limits on public notes and converts private/partial notes
/// to headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawOutputNote {
    Full(Note),
    Partial(PartialNote),
    Header(NoteHeader),
}

impl RawOutputNote {
    /// The assets contained in the note.
    pub fn assets(&self) -> Option<&NoteAssets> {
        match self {
            RawOutputNote::Full(note) => Some(note.assets()),
            RawOutputNote::Partial(note) => Some(note.assets()),
            RawOutputNote::Header(_) => None,
        }
    }

    /// Unique note identifier.
    ///
    /// This value is both an unique identifier and a commitment to the note.
    pub fn id(&self) -> NoteId {
        match self {
            RawOutputNote::Full(note) => note.id(),
            RawOutputNote::Partial(note) => note.id(),
            RawOutputNote::Header(note) => note.id(),
        }
    }

    /// Returns the recipient of the processed [`Full`](RawOutputNote::Full) output note,
    /// [`None`] if the note type is not [`Full`](RawOutputNote::Full).
    ///
    /// See [crate::note::NoteRecipient] for more details.
    pub fn recipient(&self) -> Option<&NoteRecipient> {
        match self {
            RawOutputNote::Full(note) => Some(note.recipient()),
            RawOutputNote::Partial(_) => None,
            RawOutputNote::Header(_) => None,
        }
    }

    /// Returns the recipient digest of the processed [`Full`](RawOutputNote::Full) or
    /// [`Partial`](RawOutputNote::Partial) output note. Returns [`None`] if the note type is
    /// [`Header`](RawOutputNote::Header).
    ///
    /// See [crate::note::NoteRecipient] for more details.
    pub fn recipient_digest(&self) -> Option<Word> {
        match self {
            RawOutputNote::Full(note) => Some(note.recipient().digest()),
            RawOutputNote::Partial(note) => Some(note.recipient_digest()),
            RawOutputNote::Header(_) => None,
        }
    }

    /// Note's metadata.
    pub fn metadata(&self) -> &NoteMetadata {
        match self {
            RawOutputNote::Full(note) => note.metadata(),
            RawOutputNote::Partial(note) => note.metadata(),
            RawOutputNote::Header(note) => note.metadata(),
        }
    }

    /// Converts this raw output note to a proven output note.
    ///
    /// This method performs the following transformations:
    /// - Full private notes are converted to note headers (only public info retained)
    /// - Partial notes are converted to note headers
    /// - Full public notes are wrapped in [`PublicOutputNote`], which enforces size limits
    ///
    /// # Errors
    /// Returns an error if a public note exceeds the maximum allowed size ([`NOTE_MAX_SIZE`]).
    pub fn shrink(&self) -> Result<ProvenOutputNote, PublicOutputNoteError> {
        match self {
            RawOutputNote::Full(note) if note.metadata().is_private() => {
                Ok(ProvenOutputNote::Header(*note.header()))
            },
            RawOutputNote::Full(note) => {
                let public_note = PublicOutputNote::new(note.clone())?;
                Ok(ProvenOutputNote::Public(public_note))
            },
            RawOutputNote::Partial(note) => Ok(ProvenOutputNote::Header(note.into())),
            RawOutputNote::Header(header) => Ok(ProvenOutputNote::Header(*header)),
        }
    }

    /// Returns a commitment to the note and its metadata.
    ///
    /// > hash(NOTE_ID || NOTE_METADATA)
    pub fn commitment(&self) -> Word {
        compute_note_commitment(self.id(), self.metadata())
    }
}

// CONVERSIONS
// ------------------------------------------------------------------------------------------------

impl From<RawOutputNote> for NoteHeader {
    fn from(value: RawOutputNote) -> Self {
        (&value).into()
    }
}

impl From<&RawOutputNote> for NoteHeader {
    fn from(value: &RawOutputNote) -> Self {
        match value {
            RawOutputNote::Full(note) => note.into(),
            RawOutputNote::Partial(note) => note.into(),
            RawOutputNote::Header(note) => *note,
        }
    }
}

impl From<&RawOutputNote> for NoteId {
    fn from(value: &RawOutputNote) -> Self {
        value.id()
    }
}

// SERIALIZATION
// ------------------------------------------------------------------------------------------------

impl Serializable for RawOutputNote {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        match self {
            RawOutputNote::Full(note) => {
                target.write(RAW_FULL);
                target.write(note);
            },
            RawOutputNote::Partial(note) => {
                target.write(RAW_PARTIAL);
                target.write(note);
            },
            RawOutputNote::Header(note) => {
                target.write(RAW_HEADER);
                target.write(note);
            },
        }
    }

    fn get_size_hint(&self) -> usize {
        // Serialized size of the enum tag.
        let tag_size = 0u8.get_size_hint();

        match self {
            RawOutputNote::Full(note) => tag_size + note.get_size_hint(),
            RawOutputNote::Partial(note) => tag_size + note.get_size_hint(),
            RawOutputNote::Header(note) => tag_size + note.get_size_hint(),
        }
    }
}

impl Deserializable for RawOutputNote {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match source.read_u8()? {
            RAW_FULL => Ok(RawOutputNote::Full(Note::read_from(source)?)),
            RAW_PARTIAL => Ok(RawOutputNote::Partial(PartialNote::read_from(source)?)),
            RAW_HEADER => Ok(RawOutputNote::Header(NoteHeader::read_from(source)?)),
            v => Err(DeserializationError::InvalidValue(format!(
                "invalid raw output note type: {v}"
            ))),
        }
    }
}

// PROVEN OUTPUT NOTE
// ================================================================================================

const PROVEN_PUBLIC: u8 = 0;
const PROVEN_HEADER: u8 = 1;

/// Output note types that can appear in a proven transaction.
///
/// This enum represents the final form of output notes after proving. Unlike [`RawOutputNote`],
/// this enum:
/// - Does not include partial notes (they are converted to headers)
/// - Wraps public notes in [`PublicOutputNote`] which enforces size limits
/// - Contains only the minimal information needed for verification
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvenOutputNote {
    /// A public note with full details, size-validated.
    Public(PublicOutputNote),
    /// A note header (for private notes or notes without full details).
    Header(NoteHeader),
}

impl ProvenOutputNote {
    /// Unique note identifier.
    ///
    /// This value is both an unique identifier and a commitment to the note.
    pub fn id(&self) -> NoteId {
        match self {
            ProvenOutputNote::Public(note) => note.id(),
            ProvenOutputNote::Header(header) => header.id(),
        }
    }

    /// Note's metadata.
    pub fn metadata(&self) -> &NoteMetadata {
        match self {
            ProvenOutputNote::Public(note) => note.metadata(),
            ProvenOutputNote::Header(header) => header.metadata(),
        }
    }

    /// The assets contained in the note, if available.
    ///
    /// Returns `Some` for public notes, `None` for header-only notes.
    pub fn assets(&self) -> Option<&NoteAssets> {
        match self {
            ProvenOutputNote::Public(note) => Some(note.assets()),
            ProvenOutputNote::Header(_) => None,
        }
    }

    /// Returns a commitment to the note and its metadata.
    ///
    /// > hash(NOTE_ID || NOTE_METADATA)
    pub fn commitment(&self) -> Word {
        compute_note_commitment(self.id(), self.metadata())
    }

    /// Returns the recipient of the public note, if this is a public note.
    pub fn recipient(&self) -> Option<&NoteRecipient> {
        match self {
            ProvenOutputNote::Public(note) => Some(note.recipient()),
            ProvenOutputNote::Header(_) => None,
        }
    }
}

// CONVERSIONS
// ------------------------------------------------------------------------------------------------

impl From<ProvenOutputNote> for NoteHeader {
    fn from(value: ProvenOutputNote) -> Self {
        (&value).into()
    }
}

impl From<&ProvenOutputNote> for NoteHeader {
    fn from(value: &ProvenOutputNote) -> Self {
        match value {
            ProvenOutputNote::Public(note) => note.header(),
            ProvenOutputNote::Header(header) => *header,
        }
    }
}

impl From<&ProvenOutputNote> for NoteId {
    fn from(value: &ProvenOutputNote) -> Self {
        value.id()
    }
}

// SERIALIZATION
// ------------------------------------------------------------------------------------------------

impl Serializable for ProvenOutputNote {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        match self {
            ProvenOutputNote::Public(note) => {
                target.write(PROVEN_PUBLIC);
                target.write(note);
            },
            ProvenOutputNote::Header(header) => {
                target.write(PROVEN_HEADER);
                target.write(header);
            },
        }
    }

    fn get_size_hint(&self) -> usize {
        let tag_size = 0u8.get_size_hint();
        match self {
            ProvenOutputNote::Public(note) => tag_size + note.get_size_hint(),
            ProvenOutputNote::Header(header) => tag_size + header.get_size_hint(),
        }
    }
}

impl Deserializable for ProvenOutputNote {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match source.read_u8()? {
            PROVEN_PUBLIC => Ok(ProvenOutputNote::Public(PublicOutputNote::read_from(source)?)),
            PROVEN_HEADER => Ok(ProvenOutputNote::Header(NoteHeader::read_from(source)?)),
            v => Err(DeserializationError::InvalidValue(format!(
                "invalid proven output note type: {v}"
            ))),
        }
    }
}

// PUBLIC OUTPUT NOTE
// ================================================================================================

/// A public output note with enforced size limits.
///
/// This struct wraps a [`Note`] and guarantees that:
/// - The note is public (not private)
/// - The serialized size does not exceed [`NOTE_MAX_SIZE`]
///
/// This type is used in [`ProvenOutputNote::Public`] to ensure that all public notes
/// in proven transactions meet the protocol's size requirements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicOutputNote {
    note: Note,
}

impl PublicOutputNote {
    /// Creates a new [`PublicOutputNote`] from the given note.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The note is private (use note headers for private notes)
    /// - The serialized size exceeds [`NOTE_MAX_SIZE`]
    pub fn new(note: Note) -> Result<Self, PublicOutputNoteError> {
        // Ensure the note is public
        if note.metadata().is_private() {
            return Err(PublicOutputNoteError::NoteIsPrivate(note.id()));
        }

        // Check the size limit
        let note_size = note.get_size_hint();
        if note_size > NOTE_MAX_SIZE as usize {
            return Err(PublicOutputNoteError::NoteSizeLimitExceeded {
                note_id: note.id(),
                note_size,
            });
        }

        Ok(Self { note })
    }

    /// Returns the unique identifier of this note.
    pub fn id(&self) -> NoteId {
        self.note.id()
    }

    /// Returns the note's metadata.
    pub fn metadata(&self) -> &NoteMetadata {
        self.note.metadata()
    }

    /// Returns the note's assets.
    pub fn assets(&self) -> &NoteAssets {
        self.note.assets()
    }

    /// Returns the note's recipient.
    pub fn recipient(&self) -> &NoteRecipient {
        self.note.recipient()
    }

    /// Returns the note's header.
    pub fn header(&self) -> NoteHeader {
        *self.note.header()
    }

    /// Returns a reference to the underlying note.
    pub fn note(&self) -> &Note {
        &self.note
    }

    /// Consumes this wrapper and returns the underlying note.
    pub fn into_inner(self) -> Note {
        self.note
    }
}

// SERIALIZATION
// ------------------------------------------------------------------------------------------------

impl Serializable for PublicOutputNote {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.note.write_into(target);
    }

    fn get_size_hint(&self) -> usize {
        self.note.get_size_hint()
    }
}

impl Deserializable for PublicOutputNote {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let note = Note::read_from(source)?;
        Self::new(note).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod output_notes_tests {
    use assert_matches::assert_matches;

    use super::{PublicOutputNote, RawOutputNote, RawOutputNotes};
    use crate::account::AccountId;
    use crate::assembly::Assembler;
    use crate::asset::FungibleAsset;
    use crate::note::{
        Note,
        NoteAssets,
        NoteExecutionHint,
        NoteInputs,
        NoteMetadata,
        NoteRecipient,
        NoteScript,
        NoteTag,
        NoteType,
    };
    use crate::testing::account_id::{
        ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET,
        ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET,
        ACCOUNT_ID_SENDER,
    };
    use crate::utils::serde::Serializable;
    use crate::{Felt, NOTE_MAX_SIZE, PublicOutputNoteError, TransactionOutputError, Word, ZERO};

    #[test]
    fn test_duplicate_output_notes() -> anyhow::Result<()> {
        let mock_note = Note::mock_noop(Word::empty());
        let mock_note_id = mock_note.id();
        let mock_note_clone = mock_note.clone();

        let error = RawOutputNotes::new(vec![
            RawOutputNote::Full(mock_note),
            RawOutputNote::Full(mock_note_clone),
        ])
        .expect_err("input notes creation should fail");

        assert_matches!(error, TransactionOutputError::DuplicateOutputNote(note_id) if note_id == mock_note_id);

        Ok(())
    }

    #[test]
    fn output_note_size_hint_matches_serialized_length() -> anyhow::Result<()> {
        let sender_id = ACCOUNT_ID_SENDER.try_into().unwrap();

        // Build a note with at least two assets.
        let faucet_id_1 = AccountId::try_from(ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET).unwrap();
        let faucet_id_2 = AccountId::try_from(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET).unwrap();

        let asset_1 = FungibleAsset::new(faucet_id_1, 100)?.into();
        let asset_2 = FungibleAsset::new(faucet_id_2, 200)?.into();

        let assets = NoteAssets::new(vec![asset_1, asset_2])?;

        // Build metadata similarly to how mock notes are constructed.
        let metadata = NoteMetadata::new(
            sender_id,
            NoteType::Private,
            NoteTag::from_account_id(sender_id),
            NoteExecutionHint::Always,
            ZERO,
        )?;

        // Build inputs with at least two values.
        let inputs = NoteInputs::new(vec![Felt::new(1), Felt::new(2)])?;

        let serial_num = Word::empty();
        let script = NoteScript::mock();
        let recipient = NoteRecipient::new(serial_num, script, inputs);

        let note = Note::new(assets, metadata, recipient);
        let output_note = RawOutputNote::Full(note);

        let bytes = output_note.to_bytes();

        assert_eq!(bytes.len(), output_note.get_size_hint());

        Ok(())
    }

    #[test]
    fn oversized_public_note_triggers_size_limit_error() -> anyhow::Result<()> {
        // Construct a public note whose serialized size exceeds NOTE_MAX_SIZE by creating
        // a very large note script so that the script's serialized MAST alone is larger
        // than the configured limit.

        let sender_id = ACCOUNT_ID_SENDER.try_into().unwrap();

        // Build a large MASM program with many `nop` instructions.
        let mut src = alloc::string::String::from("begin\n");
        // The exact threshold is not critical as long as we clearly exceed NOTE_MAX_SIZE.
        for _ in 0..5000 {
            src.push_str("    nop\n");
        }
        src.push_str("end\n");

        let assembler = Assembler::default();
        let program = assembler.assemble_program(&src).unwrap();
        let script = NoteScript::new(program);

        let serial_num = Word::empty();
        let inputs = NoteInputs::new(alloc::vec::Vec::new())?;

        // Create a public note (NoteType::Public is required for PublicOutputNote)
        let faucet_id = AccountId::try_from(ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET).unwrap();
        let asset = FungibleAsset::new(faucet_id, 100)?.into();
        let assets = NoteAssets::new(vec![asset])?;

        let metadata = NoteMetadata::new(
            sender_id,
            NoteType::Public,
            NoteTag::from_account_id(sender_id),
            NoteExecutionHint::Always,
            ZERO,
        )?;

        let recipient = NoteRecipient::new(serial_num, script, inputs);
        let oversized_note = Note::new(assets, metadata, recipient);

        // Sanity-check that our constructed note is indeed larger than the configured
        // maximum.
        let computed_note_size = oversized_note.get_size_hint();
        assert!(computed_note_size > NOTE_MAX_SIZE as usize);

        // Creating a PublicOutputNote should fail with size limit error
        let result = PublicOutputNote::new(oversized_note);

        assert_matches!(
            result,
            Err(PublicOutputNoteError::NoteSizeLimitExceeded { note_id: _, note_size })
                if note_size > NOTE_MAX_SIZE as usize
        );

        Ok(())
    }

    #[test]
    fn shrink_enforces_size_limit_on_public_notes() -> anyhow::Result<()> {
        // Test that shrink() on a RawOutputNote with an oversized public note fails

        let sender_id = ACCOUNT_ID_SENDER.try_into().unwrap();

        // Build a large MASM program
        let mut src = alloc::string::String::from("begin\n");
        for _ in 0..5000 {
            src.push_str("    nop\n");
        }
        src.push_str("end\n");

        let assembler = Assembler::default();
        let program = assembler.assemble_program(&src).unwrap();
        let script = NoteScript::new(program);

        let serial_num = Word::empty();
        let inputs = NoteInputs::new(alloc::vec::Vec::new())?;

        let faucet_id = AccountId::try_from(ACCOUNT_ID_PRIVATE_FUNGIBLE_FAUCET).unwrap();
        let asset = FungibleAsset::new(faucet_id, 100)?.into();
        let assets = NoteAssets::new(vec![asset])?;

        // Create a PUBLIC note so it will go through size validation during shrink
        let metadata = NoteMetadata::new(
            sender_id,
            NoteType::Public,
            NoteTag::from_account_id(sender_id),
            NoteExecutionHint::Always,
            ZERO,
        )?;

        let recipient = NoteRecipient::new(serial_num, script, inputs);
        let oversized_note = Note::new(assets, metadata, recipient);
        let raw_output_note = RawOutputNote::Full(oversized_note);

        // Shrink should fail because the public note exceeds size limit
        let result = raw_output_note.shrink();

        assert_matches!(
            result,
            Err(PublicOutputNoteError::NoteSizeLimitExceeded { note_id: _, note_size })
                if note_size > NOTE_MAX_SIZE as usize
        );

        Ok(())
    }
}

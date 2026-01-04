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
use crate::{Felt, Hasher, MAX_OUTPUT_NOTES_PER_TX, NOTE_MAX_SIZE, TransactionOutputError, Word};

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
    pub output_notes: OutputNotes,
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
        let output_notes = OutputNotes::read_from(source)?;
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputNotes {
    notes: Vec<OutputNote>,
    commitment: Word,
}

impl OutputNotes {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// Returns new [OutputNotes] instantiated from the provide vector of notes.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The total number of notes is greater than [`MAX_OUTPUT_NOTES_PER_TX`].
    /// - The vector of notes contains duplicates.
    /// - Any individual output note exceeds the maximum allowed serialized size [`NOTE_MAX_SIZE`].
    pub fn new(notes: Vec<OutputNote>) -> Result<Self, TransactionOutputError> {
        if notes.len() > MAX_OUTPUT_NOTES_PER_TX {
            return Err(TransactionOutputError::TooManyOutputNotes(notes.len()));
        }

        let mut seen_notes = BTreeSet::new();
        for note in notes.iter() {
            let note_id = note.id();
            if !seen_notes.insert(note_id) {
                return Err(TransactionOutputError::DuplicateOutputNote(note_id));
            }

            let note_size = note.get_size_hint();
            if note_size > NOTE_MAX_SIZE as usize {
                return Err(TransactionOutputError::OutputNoteSizeLimitExceeded {
                    note_id,
                    note_size,
                });
            }
        }

        let commitment = Self::compute_commitment(notes.iter().map(NoteHeader::from));

        Ok(Self { notes, commitment })
    }

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
    pub fn get_note(&self, idx: usize) -> &OutputNote {
        &self.notes[idx]
    }

    // ITERATORS
    // --------------------------------------------------------------------------------------------

    /// Returns an iterator over notes in this [OutputNotes].
    pub fn iter(&self) -> impl Iterator<Item = &OutputNote> {
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

// SERIALIZATION
// ------------------------------------------------------------------------------------------------

impl Serializable for OutputNotes {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        // assert is OK here because we enforce max number of notes in the constructor
        assert!(self.notes.len() <= u16::MAX.into());
        target.write_u16(self.notes.len() as u16);
        target.write_many(&self.notes);
    }
}

impl Deserializable for OutputNotes {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let num_notes = source.read_u16()?;
        let notes = source.read_many::<OutputNote>(num_notes.into())?;
        Self::new(notes).map_err(|err| DeserializationError::InvalidValue(err.to_string()))
    }
}

// OUTPUT NOTE
// ================================================================================================

const FULL: u8 = 0;
const PARTIAL: u8 = 1;
const HEADER: u8 = 2;

/// The types of note outputs supported by the transaction kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputNote {
    Full(Note),
    Partial(PartialNote),
    Header(NoteHeader),
}

impl OutputNote {
    /// The assets contained in the note.
    pub fn assets(&self) -> Option<&NoteAssets> {
        match self {
            OutputNote::Full(note) => Some(note.assets()),
            OutputNote::Partial(note) => Some(note.assets()),
            OutputNote::Header(_) => None,
        }
    }

    /// Unique note identifier.
    ///
    /// This value is both an unique identifier and a commitment to the note.
    pub fn id(&self) -> NoteId {
        match self {
            OutputNote::Full(note) => note.id(),
            OutputNote::Partial(note) => note.id(),
            OutputNote::Header(note) => note.id(),
        }
    }

    /// Returns the recipient of the processed [`Full`](OutputNote::Full) output note, [`None`] if
    /// the note type is not [`Full`](OutputNote::Full).
    ///
    /// See [crate::note::NoteRecipient] for more details.
    pub fn recipient(&self) -> Option<&NoteRecipient> {
        match self {
            OutputNote::Full(note) => Some(note.recipient()),
            OutputNote::Partial(_) => None,
            OutputNote::Header(_) => None,
        }
    }

    /// Returns the recipient digest of the processed [`Full`](OutputNote::Full) or
    /// [`Partial`](OutputNote::Partial) output note. Returns [`None`] if the note type is
    /// [`Header`](OutputNote::Header).
    ///
    /// See [crate::note::NoteRecipient] for more details.
    pub fn recipient_digest(&self) -> Option<Word> {
        match self {
            OutputNote::Full(note) => Some(note.recipient().digest()),
            OutputNote::Partial(note) => Some(note.recipient_digest()),
            OutputNote::Header(_) => None,
        }
    }

    /// Note's metadata.
    pub fn metadata(&self) -> &NoteMetadata {
        match self {
            OutputNote::Full(note) => note.metadata(),
            OutputNote::Partial(note) => note.metadata(),
            OutputNote::Header(note) => note.metadata(),
        }
    }

    /// Erase private note information.
    ///
    /// Specifically:
    /// - Full private notes are converted into note headers.
    /// - All partial notes are converted into note headers.
    pub fn shrink(&self) -> Self {
        match self {
            OutputNote::Full(note) if note.metadata().is_private() => {
                OutputNote::Header(*note.header())
            },
            OutputNote::Partial(note) => OutputNote::Header(note.into()),
            _ => self.clone(),
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

impl From<OutputNote> for NoteHeader {
    fn from(value: OutputNote) -> Self {
        (&value).into()
    }
}

impl From<&OutputNote> for NoteHeader {
    fn from(value: &OutputNote) -> Self {
        match value {
            OutputNote::Full(note) => note.into(),
            OutputNote::Partial(note) => note.into(),
            OutputNote::Header(note) => *note,
        }
    }
}

// SERIALIZATION
// ------------------------------------------------------------------------------------------------

impl Serializable for OutputNote {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        match self {
            OutputNote::Full(note) => {
                target.write(FULL);
                target.write(note);
            },
            OutputNote::Partial(note) => {
                target.write(PARTIAL);
                target.write(note);
            },
            OutputNote::Header(note) => {
                target.write(HEADER);
                target.write(note);
            },
        }
    }

    fn get_size_hint(&self) -> usize {
        // Serialized size of the enum tag.
        let tag_size = 0u8.get_size_hint();

        match self {
            OutputNote::Full(note) => tag_size + note.get_size_hint(),
            OutputNote::Partial(note) => tag_size + note.get_size_hint(),
            OutputNote::Header(note) => tag_size + note.get_size_hint(),
        }
    }
}

impl Deserializable for OutputNote {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match source.read_u8()? {
            FULL => Ok(OutputNote::Full(Note::read_from(source)?)),
            PARTIAL => Ok(OutputNote::Partial(PartialNote::read_from(source)?)),
            HEADER => Ok(OutputNote::Header(NoteHeader::read_from(source)?)),
            v => Err(DeserializationError::InvalidValue(format!("invalid note type: {v}"))),
        }
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod output_notes_tests {
    use assert_matches::assert_matches;

    use super::OutputNotes;
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
    use crate::transaction::OutputNote;
    use crate::utils::serde::Serializable;
    use crate::{Felt, NOTE_MAX_SIZE, TransactionOutputError, Word, ZERO};

    #[test]
    fn test_duplicate_output_notes() -> anyhow::Result<()> {
        let mock_note = Note::mock_noop(Word::empty());
        let mock_note_id = mock_note.id();
        let mock_note_clone = mock_note.clone();

        let error =
            OutputNotes::new(vec![OutputNote::Full(mock_note), OutputNote::Full(mock_note_clone)])
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
        let output_note = OutputNote::Full(note);

        let bytes = output_note.to_bytes();

        assert_eq!(bytes.len(), output_note.get_size_hint());

        Ok(())
    }

    #[test]
    fn oversized_output_note_triggers_size_limit_error() -> anyhow::Result<()> {
        // Construct a note whose serialized size exceeds NOTE_MAX_SIZE by creating a very
        // large note script (many instructions) so that the script's serialized MAST alone
        // is larger than the configured limit.

        // Build a large MASM program with many `nop` instructions.
        let mut src = alloc::string::String::from("begin\n");
        // The exact threshold is not critical as long as we clearly exceed NOTE_MAX_SIZE.
        // Thousands of instructions are cheap enough for a test but large enough to
        // produce a big MAST.
        for _ in 0..5000 {
            src.push_str("    nop\n");
        }
        src.push_str("end\n");

        let assembler = Assembler::default();
        let program = assembler.assemble_program(&src).unwrap();
        let script = NoteScript::new(program);

        let serial_num = Word::empty();
        let inputs = NoteInputs::new(alloc::vec::Vec::new())?;

        let base_note = Note::mock_noop(Word::empty());
        let assets = base_note.assets().clone();
        let metadata = *base_note.header().metadata();

        let recipient = NoteRecipient::new(serial_num, script, inputs);
        let oversized_note = Note::new(assets, metadata, recipient);
        let oversized_output_note = OutputNote::Full(oversized_note);

        // Sanity-check that our constructed note is indeed larger than the configured
        // maximum. If this ever fails, it likely means the protocol constants or
        // serialization format have changed.
        let computed_note_size = oversized_output_note.get_size_hint();
        assert!(computed_note_size > NOTE_MAX_SIZE as usize);

        let result = OutputNotes::new(vec![oversized_output_note]);

        assert_matches!(
            result,
            Err(TransactionOutputError::OutputNoteSizeLimitExceeded { note_id: _, note_size })
                if note_size > NOTE_MAX_SIZE as usize
        );

        Ok(())
    }
}

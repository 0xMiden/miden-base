use alloc::vec::Vec;

use crate::block::{
    BlockAccountUpdate,
    BlockBody,
    BlockHeader,
    BlockNoteIndex,
    BlockNoteTree,
    OutputNoteBatch,
};
use crate::note::Nullifier;
use crate::transaction::{OrderedTransactionHeaders, OutputNote};
use crate::utils::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable};
use crate::{MIN_PROOF_SECURITY_LEVEL, Word};

// PROVEN BLOCK
// ================================================================================================

/// A block in the Miden chain.
///
/// A block is built from batches of transactions, i.e. multiple
/// [`ProvenBatch`](crate::batch::ProvenBatch)es, and each batch contains multiple
/// [`ProvenTransaction`](crate::transaction::ProvenTransaction)s.
///
/// It consists of the following components:
/// - A [`BlockHeader`] committing to the current state of the chain and against which account, note
///   or nullifier inclusion or absence can be proven. See its documentation for details on what it
///   commits to. Eventually, it will also contain a ZK proof of the validity of the block.
/// - A list of account updates for all accounts updated in this block. For private accounts, the
///   update contains only the new account state commitments while for public accounts, the update
///   also includes the delta which can be applied to the previous account state to get the new
///   account state.
/// - A list of new notes created in this block. For private notes, the block contains only note IDs
///   and note metadata while for public notes the full note details are included.
/// - A list of new nullifiers created for all notes that were consumed in the block.
/// - A list of transaction headers that were included in the block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenBlock {
    /// The header of the block, committing to the current state of the chain.
    header: BlockHeader,

    /// The body of the block, containing the transactions and state updates.
    body: BlockBody,
}

impl ProvenBlock {
    /// Returns a new [`ProvenBlock`] instantiated from the provided components.
    ///
    /// # Warning
    ///
    /// This constructor does not do any validation, so passing incorrect values may lead to later
    /// panics.
    pub fn new_unchecked(header: BlockHeader, body: BlockBody) -> Self {
        Self { header, body }
    }

    /// Returns the commitment to this block.
    pub fn commitment(&self) -> Word {
        self.header.commitment()
    }

    /// Returns the header of this block.
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    /// Returns the slice of [`BlockAccountUpdate`]s for all accounts updated in this block.
    pub fn updated_accounts(&self) -> &[BlockAccountUpdate] {
        self.body.updated_accounts()
    }

    /// Returns the slice of [`OutputNoteBatch`]es for all output notes created in this block.
    pub fn output_note_batches(&self) -> &[OutputNoteBatch] {
        self.body.output_note_batches()
    }

    /// Returns the proof security level of the block.
    pub fn proof_security_level(&self) -> u32 {
        MIN_PROOF_SECURITY_LEVEL
    }

    /// Returns an iterator over all [`OutputNote`]s created in this block.
    ///
    /// Each note is accompanied by a corresponding index specifying where the note is located
    /// in the block's [`BlockNoteTree`].
    pub fn output_notes(&self) -> impl Iterator<Item = (BlockNoteIndex, &OutputNote)> {
        self.body
            .output_note_batches()
            .iter()
            .enumerate()
            .flat_map(|(batch_idx, notes)| {
                notes.iter().map(move |(note_idx_in_batch, note)| {
                    (
                        // SAFETY: The proven block contains at most the max allowed number of
                        // batches and each batch is guaranteed to contain
                        // at most the max allowed number of output notes.
                        BlockNoteIndex::new(batch_idx, *note_idx_in_batch).expect(
                            "max batches in block and max notes in batches should be enforced",
                        ),
                        note,
                    )
                })
            })
    }

    /// Returns the [`BlockNoteTree`] containing all [`OutputNote`]s created in this block.
    pub fn build_output_note_tree(&self) -> BlockNoteTree {
        let entries = self
            .output_notes()
            .map(|(note_index, note)| (note_index, note.id(), *note.metadata()));

        // SAFETY: We only construct proven blocks that:
        // - do not contain duplicates
        // - contain at most the max allowed number of batches and each batch is guaranteed to
        //   contain at most the max allowed number of output notes.
        BlockNoteTree::with_entries(entries)
            .expect("the output notes of the block should not contain duplicates and contain at most the allowed maximum")
    }

    /// Returns a reference to the slice of nullifiers for all notes consumed in the block.
    pub fn created_nullifiers(&self) -> &[Nullifier] {
        self.body.created_nullifiers()
    }

    /// Returns the [`OrderedTransactionHeaders`] of all transactions included in this block.
    pub fn transactions(&self) -> &OrderedTransactionHeaders {
        self.body.transactions()
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for ProvenBlock {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.header.write_into(target);
        self.body.write_into(target);
    }
}

impl Deserializable for ProvenBlock {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block = Self {
            header: BlockHeader::read_from(source)?,
            body: BlockBody::read_from(source)?,
        };

        Ok(block)
    }
}

// TESTING
// ================================================================================================

#[cfg(any(feature = "testing", test))]
impl ProvenBlock {
    /// Returns a mutable reference to the block's account updates for testing purposes.
    pub fn updated_accounts_mut(&mut self) -> &mut Vec<BlockAccountUpdate> {
        self.body.updated_accounts_mut()
    }

    /// Returns a mutable reference to the block's nullifiers for testing purposes.
    pub fn created_nullifiers_mut(&mut self) -> &mut Vec<Nullifier> {
        self.body.created_nullifiers_mut()
    }

    /// Returns a mutable reference to the block's output note batches for testing purposes.
    pub fn output_note_batches_mut(&mut self) -> &mut Vec<OutputNoteBatch> {
        self.body.output_note_batches_mut()
    }

    /// Sets the block's header for testing purposes.
    pub fn set_block_header(&mut self, header: BlockHeader) {
        self.header = header;
    }
}

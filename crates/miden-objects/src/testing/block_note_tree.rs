use miden_crypto::merkle::MerkleError;

use crate::block::{BlockNoteIndex, BlockNoteTree, OutputNoteBatch};

impl BlockNoteTree {
    /// Creates a [`BlockNoteTree`] from output note batches.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - The provided batch or note indices are out of bounds.
    ///
    /// # Errors
    ///
    /// Identical to [`BlockNoteTree::with_entries`].
    pub fn from_note_batches(notes: &[OutputNoteBatch]) -> Result<BlockNoteTree, MerkleError> {
        let iter = notes.iter().enumerate().flat_map(|(batch_idx, batch_notes)| {
            batch_notes.iter().map(move |(note_idx_in_batch, note)| {
                let block_note_index = BlockNoteIndex::new(batch_idx, *note_idx_in_batch)
                    .expect("max batches in block and max notes in batches should be enforced");
                (block_note_index, note.id(), *note.metadata())
            })
        });

        Self::with_entries(iter)
    }
}

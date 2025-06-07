use super::{
    ByteReader, ByteWriter, Deserializable, DeserializationError, NoteError, Serializable,
};
use crate::{
    MAX_BATCHES_PER_BLOCK, MAX_OUTPUT_NOTES_PER_BATCH, block::BlockNumber,
    crypto::merkle::MerklePath,
};

/// Contains information about the location of a note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteLocation {
    /// The block number the note was created in.
    block_num: BlockNumber,

    /// The index of the note in the note Merkle tree of the block the note was created in.
    node_index_in_block: u16,
}

impl NoteLocation {
    /// Returns the block number the note was created in.
    pub fn block_num(&self) -> BlockNumber {
        self.block_num
    }

    /// Returns the index of the note in the note Merkle tree of the block the note was created in.
    ///
    /// # Note
    ///
    /// The height of the Merkle tree is [crate::constants::BLOCK_NOTE_TREE_DEPTH].
    /// Thus, the maximum index is `2 ^ BLOCK_NOTE_TREE_DEPTH - 1`.
    pub fn node_index_in_block(&self) -> u16 {
        self.node_index_in_block
    }
}

/// Contains the data required to prove inclusion of a note in the canonical chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteInclusionProof {
    /// Details about the note's location.
    location: NoteLocation,

    /// The note's authentication Merkle path its block's the note root.
    note_path: MerklePath,
}

impl NoteInclusionProof {
    /// Returns a new [NoteInclusionProof].
    pub fn new(
        block_num: BlockNumber,
        node_index_in_block: u16,
        note_path: MerklePath,
    ) -> Result<Self, NoteError> {
        const HIGHEST_INDEX: usize = MAX_BATCHES_PER_BLOCK * MAX_OUTPUT_NOTES_PER_BATCH - 1;
        if node_index_in_block as usize > HIGHEST_INDEX {
            return Err(NoteError::NoteLocationIndexOutOfBounds {
                node_index_in_block,
                highest_index: HIGHEST_INDEX,
            });
        }

        let location = NoteLocation { block_num, node_index_in_block };

        Ok(Self { location, note_path })
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the location of the note.
    pub fn location(&self) -> &NoteLocation {
        &self.location
    }

    /// Returns the index of the merkle tree leaf in the note tree.
    pub fn location_index(&self) -> u16 {
        self.location.node_index_in_block()
    }

    /// Returns the Merkle path to the note in the note Merkle tree of the block the note was
    /// created in.
    pub fn note_path(&self) -> &MerklePath {
        &self.note_path
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for NoteLocation {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        target.write(self.block_num);
        target.write_u16(self.node_index_in_block);
    }
}

impl Deserializable for NoteLocation {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block_num = source.read()?;
        let node_index_in_block = source.read_u16()?;

        Ok(Self { block_num, node_index_in_block })
    }
}

impl Serializable for NoteInclusionProof {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.location.write_into(target);
        self.note_path.write_into(target);
    }
}

impl Deserializable for NoteInclusionProof {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let location = NoteLocation::read_from(source)?;
        let note_path = MerklePath::read_from(source)?;

        Ok(Self { location, note_path })
    }
}

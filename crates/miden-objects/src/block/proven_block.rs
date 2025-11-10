use crate::MIN_PROOF_SECURITY_LEVEL;
use crate::block::{BlockBody, BlockHeader, BlockProof, SignedBlock};
use crate::utils::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable};

// PROVEN BLOCK
// ================================================================================================

/// Represents a block in the Miden blockchain that has been signed and proven.
///
/// Blocks transition from proposed, signed, and proven states. This struct represents the final,
/// proven state of a block.
///
/// Proven blocks are the final, canonical blocks in the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenBlock {
    /// The signed block that the [`BlockProof`] is based on.
    signed_block: SignedBlock,

    /// The proof of the block.
    block_proof: BlockProof,
}

impl ProvenBlock {
    /// Returns a new [`ProvenBlock`] instantiated from the provided components.
    ///
    /// # Warning
    ///
    /// This constructor does not do any validation, so passing incorrect values may lead to later
    /// panics.
    pub fn new_unchecked(signed_block: SignedBlock, block_proof: BlockProof) -> Self {
        Self { signed_block, block_proof }
    }

    /// Returns the proof security level of the block.
    pub fn proof_security_level(&self) -> u32 {
        MIN_PROOF_SECURITY_LEVEL
    }

    /// Returns the header of the block.
    pub fn header(&self) -> &BlockHeader {
        self.signed_block.header()
    }

    /// Returns the body of the block.
    pub fn body(&self) -> &BlockBody {
        self.signed_block.body()
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for ProvenBlock {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.signed_block.write_into(target);
        self.block_proof.write_into(target);
    }
}

impl Deserializable for ProvenBlock {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block = Self {
            signed_block: SignedBlock::read_from(source)?,
            block_proof: BlockProof::read_from(source)?,
        };

        Ok(block)
    }
}

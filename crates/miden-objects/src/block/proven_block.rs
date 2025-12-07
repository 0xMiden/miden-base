use miden_crypto::dsa::ecdsa_k256_keccak::Signature;

use crate::MIN_PROOF_SECURITY_LEVEL;
use crate::block::{BlockBody, BlockHeader, BlockProof};
use crate::utils::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable};

// PROVEN BLOCK
// ================================================================================================

/// Represents a block in the Miden blockchain that has been signed and proven.
///
/// Blocks transition through proposed, signed, and proven states. This struct represents the final,
/// proven state of a block.
///
/// Proven blocks are the final, canonical blocks in the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenBlock {
    /// The header of the proven block.
    header: BlockHeader,

    /// Signature of the block header.
    signature: Signature,

    /// The body of the proven block.
    body: BlockBody,

    /// The proof of the block.
    proof: BlockProof,
}

impl ProvenBlock {
    /// Returns a new [`ProvenBlock`] instantiated from the provided components.
    ///
    /// # Warning
    ///
    /// This constructor does not do any validation, so passing incorrect values may lead to later
    /// panics.
    pub fn new_unchecked(
        header: BlockHeader,
        signature: Signature,
        body: BlockBody,
        proof: BlockProof,
    ) -> Self {
        Self { header, signature, body, proof }
    }

    /// Returns the proof security level of the block.
    pub fn proof_security_level(&self) -> u32 {
        MIN_PROOF_SECURITY_LEVEL
    }

    /// Returns the header of the block.
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    /// Returns the signature of the block header.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Returns the body of the block.
    pub fn body(&self) -> &BlockBody {
        &self.body
    }

    /// Returns the proof of the block.
    pub fn proof(&self) -> &BlockProof {
        &self.proof
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for ProvenBlock {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.header.write_into(target);
        self.signature.write_into(target);
        self.body.write_into(target);
        self.proof.write_into(target);
    }
}

impl Deserializable for ProvenBlock {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block = Self {
            header: BlockHeader::read_from(source)?,
            signature: Signature::read_from(source)?,
            body: BlockBody::read_from(source)?,
            proof: BlockProof::read_from(source)?,
        };

        Ok(block)
    }
}

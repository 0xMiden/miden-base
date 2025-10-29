use core::ops::{Deref, DerefMut};

use miden_core::utils::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use miden_crypto::dsa::ecdsa_k256_keccak::{PublicKey, Signature};

use crate::block::{BlockHeader, ProposedBlock};

#[derive(Debug, Clone)]
pub struct SignedBlock {
    header: BlockHeader,
    proposed_block: ProposedBlock,
    signature: Signature,
}

impl Deref for SignedBlock {
    type Target = ProposedBlock;

    fn deref(&self) -> &Self::Target {
        &self.proposed_block
    }
}

impl DerefMut for SignedBlock {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.proposed_block
    }
}

impl SignedBlock {
    /// Creates a new signed block with the given proven block and signature.
    pub fn new(header: BlockHeader, proposed_block: ProposedBlock, signature: Signature) -> Self {
        SignedBlock { header, proposed_block, signature }
    }

    /// Returns a reference to the signature of the signed block.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Verifies the signature of the signed block.
    pub fn verify(&self, pub_key: &PublicKey) -> bool {
        self.signature.verify(self.header.commitment(), pub_key) // TODO: what do we sign/verify?
    }

    /// Consumes the signed block and returns its parts.
    pub fn into_parts(self) -> (BlockHeader, ProposedBlock, Signature) {
        (self.header, self.proposed_block, self.signature)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for SignedBlock {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.header.write_into(target);
        self.proposed_block.write_into(target);
        self.signature.write_into(target);
    }
}

impl Deserializable for SignedBlock {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block = Self {
            header: BlockHeader::read_from(source)?,
            proposed_block: ProposedBlock::read_from(source)?,
            signature: Signature::read_from(source)?,
        };
        Ok(block)
    }
}

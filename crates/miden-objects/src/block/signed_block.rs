use core::ops::Deref;

use miden_core::utils::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};
use miden_crypto::dsa::ecdsa_k256_keccak::{PublicKey, Signature};

use crate::block::ProvenBlock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedBlock {
    proven_block: ProvenBlock,
    signature: Signature,
}

impl Deref for SignedBlock {
    type Target = ProvenBlock;

    fn deref(&self) -> &Self::Target {
        &self.proven_block
    }
}

impl SignedBlock {
    /// Creates a new signed block with the given proven block and signature.
    ///
    /// This should only be used internally by the [`ProvenBlock`] struct.
    pub(crate) fn new(proven_block: ProvenBlock, signature: Signature) -> Self {
        SignedBlock { proven_block, signature }
    }

    /// Returns a reference to the signature of the signed block.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Verifies the signature of the signed block.
    pub fn verify(&self, pub_key: &PublicKey) -> bool {
        self.signature.verify(self.proven_block.commitment(), pub_key)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for SignedBlock {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.proven_block.write_into(target);
        self.signature.write_into(target);
    }
}

impl Deserializable for SignedBlock {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block = Self {
            proven_block: ProvenBlock::read_from(source)?,
            signature: Signature::read_from(source)?,
        };
        Ok(block)
    }
}

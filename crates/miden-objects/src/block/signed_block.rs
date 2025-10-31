use miden_core::utils::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};

use crate::block::{BlockBody, BlockHeader};

#[derive(Debug, Clone)]
pub struct SignedBlock {
    header: BlockHeader,
    body: BlockBody,
}

impl SignedBlock {
    /// Creates a new [`SignedBlock`] with the given header and body.
    pub fn new(header: BlockHeader, body: BlockBody) -> Self {
        SignedBlock { header, body }
    }

    /// Consumes the signed block and returns its parts.
    pub fn into_parts(self) -> (BlockHeader, BlockBody) {
        (self.header, self.body)
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for SignedBlock {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.header.write_into(target);
        self.body.write_into(target);
    }
}

impl Deserializable for SignedBlock {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let block = Self {
            header: BlockHeader::read_from(source)?,
            body: BlockBody::read_from(source)?,
        };
        Ok(block)
    }
}

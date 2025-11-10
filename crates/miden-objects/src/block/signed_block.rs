use miden_core::utils::{
    ByteReader,
    ByteWriter,
    Deserializable,
    DeserializationError,
    Serializable,
};

use crate::block::{BlockBody, BlockHeader};

/// Represents a block in the Miden blockchain that has been signed by the designated validator.
///
/// Blocks transition from proposed, signed, and proven states. This struct represents the signed
/// state of a block which can be used to then create a proven block.
///
/// Signed blocks are intended to be treated as finalized blocks in the chain. If the network cannot
/// proven a previously signed block, it is treated as a re-org event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedBlock {
    header: BlockHeader,
    body: BlockBody,
}

impl SignedBlock {
    /// Creates a new [`SignedBlock`] with the given header and body.
    ///
    /// # Warning
    ///
    /// This constructor does not do any validation, so passing incorrect values may lead to later
    /// panics.
    pub fn new_unchecked(header: BlockHeader, body: BlockBody) -> Self {
        SignedBlock { header, body }
    }

    /// Returns the header of the signed block.
    pub fn header(&self) -> &BlockHeader {
        &self.header
    }

    /// Returns the body of the signed block.
    pub fn body(&self) -> &BlockBody {
        &self.body
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

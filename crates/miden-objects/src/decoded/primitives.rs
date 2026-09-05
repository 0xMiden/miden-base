//! Domain construction for decoded primitives messages.
pub use proto::primitives::DecodedMerklePath as MerklePath;

use crate::{ConversionError, DecodeMessage, Verify, proto};

impl Verify for MerklePath {
    type Verified = miden_protocol::crypto::merkle::MerklePath;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.siblings))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::MerklePath> for miden_protocol::crypto::merkle::MerklePath {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::MerklePath) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedSparseMerklePath as SparseMerklePath;

impl Verify for SparseMerklePath {
    type Verified = miden_protocol::crypto::merkle::SparseMerklePath;
    type Error = miden_protocol::crypto::merkle::MerkleError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Self::Verified::from_parts(self.empty_nodes_mask, self.siblings)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::SparseMerklePath>
    for miden_protocol::crypto::merkle::SparseMerklePath
{
    type Error = ConversionError;
    fn try_from(value: proto::primitives::SparseMerklePath) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedSmtLeafEntry as SmtLeafEntry;

impl Verify for SmtLeafEntry {
    type Verified = (miden_protocol::Word, miden_protocol::Word);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.key, self.value))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::SmtLeafEntry> for (miden_protocol::Word, miden_protocol::Word) {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::SmtLeafEntry) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedPartialSmtNode as PartialSmtNode;

impl Verify for PartialSmtNode {
    type Verified = (u64, miden_protocol::Word);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.index, self.digest))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::PartialSmtNode> for (u64, miden_protocol::Word) {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::PartialSmtNode) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedPartialSmtNodeLevel as PartialSmtNodeLevel;

impl Verify for PartialSmtNodeLevel {
    type Verified = (u32, alloc::vec::Vec<(u64, miden_protocol::Word)>);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((
            self.depth,
            self.nodes.into_iter().map(Verify::verify).collect::<Result<_, _>>()?,
        ))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::PartialSmtNodeLevel>
    for (u32, alloc::vec::Vec<(u64, miden_protocol::Word)>)
{
    type Error = ConversionError;
    fn try_from(value: proto::primitives::PartialSmtNodeLevel) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedIndexedDigest as IndexedDigest;

impl Verify for IndexedDigest {
    type Verified = (u64, miden_protocol::Word);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.index, self.value))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::IndexedDigest> for (u64, miden_protocol::Word) {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::IndexedDigest) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedSmtLeafEntryList as SmtLeafEntryList;

impl Verify for SmtLeafEntryList {
    type Verified = alloc::vec::Vec<(miden_protocol::Word, miden_protocol::Word)>;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        self.entries.into_iter().map(Verify::verify).collect()
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::SmtLeafEntryList>
    for alloc::vec::Vec<(miden_protocol::Word, miden_protocol::Word)>
{
    type Error = ConversionError;
    fn try_from(value: proto::primitives::SmtLeafEntryList) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedMerkleStoreNode as MerkleStoreNode;

impl Verify for MerkleStoreNode {
    type Verified = miden_protocol::crypto::merkle::InnerNodeInfo;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified {
            value: self.value,
            left: self.left,
            right: self.right,
        })
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::MerkleStoreNode> for miden_protocol::crypto::merkle::InnerNodeInfo {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::MerkleStoreNode) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedAdviceMapEntry as AdviceMapEntry;

impl Verify for AdviceMapEntry {
    type Verified = (miden_protocol::Word, alloc::vec::Vec<miden_protocol::Felt>);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.key, self.values))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::AdviceMapEntry>
    for (miden_protocol::Word, alloc::vec::Vec<miden_protocol::Felt>)
{
    type Error = ConversionError;
    fn try_from(value: proto::primitives::AdviceMapEntry) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedAdviceStack as AdviceStack;

impl Verify for AdviceStack {
    type Verified = miden_protocol::vm::AdviceStack;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(self.values.into_iter().collect())
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::AdviceStack> for miden_protocol::vm::AdviceStack {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::AdviceStack) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

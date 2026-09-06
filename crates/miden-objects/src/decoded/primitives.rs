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

pub use proto::primitives::DecodedAdviceMap as AdviceMap;

impl Verify for AdviceMap {
    type Verified = miden_protocol::vm::AdviceMap;
    type Error = AdviceError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let mut entries = alloc::collections::BTreeMap::new();
        for entry in self.entries {
            if entries.insert(entry.key, entry.values).is_some() {
                return Err(AdviceError::DuplicateMapKey(entry.key));
            }
        }
        Ok(entries.into())
    }
}
#[derive(Debug, thiserror::Error)]
pub enum AdviceError {
    #[error("duplicate advice map key {0}")]
    DuplicateMapKey(miden_protocol::Word),
    #[error("duplicate Merkle store parent {0}")]
    DuplicateMerkleParent(miden_protocol::Word),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::AdviceMap> for miden_protocol::vm::AdviceMap {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::AdviceMap) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedMerkleStore as MerkleStore;

impl Verify for MerkleStore {
    type Verified = miden_protocol::crypto::merkle::store::MerkleStore;
    type Error = AdviceError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let mut nodes = alloc::collections::BTreeMap::new();
        for node in self.nodes {
            let node = node.verify().expect("infallible Merkle store node");
            if nodes.insert(node.value, node.clone()).is_some() {
                return Err(AdviceError::DuplicateMerkleParent(node.value));
            }
        }
        let mut store = Self::Verified::new();
        store.extend(nodes.into_values());
        Ok(store)
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::MerkleStore>
    for miden_protocol::crypto::merkle::store::MerkleStore
{
    type Error = ConversionError;
    fn try_from(value: proto::primitives::MerkleStore) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedAdviceInputs as AdviceInputs;

impl Verify for AdviceInputs {
    type Verified = miden_protocol::vm::AdviceInputs;
    type Error = AdviceError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.advice_stack.verify().expect("infallible advice stack"),
            self.advice_map.verify()?,
            self.merkle_store.verify()?,
        ))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::AdviceInputs> for miden_protocol::vm::AdviceInputs {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::AdviceInputs) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedMmrDelta as MmrDelta;

impl Verify for MmrDelta {
    type Verified = miden_protocol::crypto::merkle::mmr::MmrDelta;
    type Error = MmrDeltaError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let forest = miden_protocol::crypto::merkle::mmr::Forest::new(self.forest.try_into()?)?;
        Ok(Self::Verified { forest, data: self.update_data })
    }
}
#[derive(Debug, thiserror::Error)]
pub enum MmrDeltaError {
    #[error("forest size does not fit in usize: {0}")]
    Size(#[from] core::num::TryFromIntError),
    #[error("forest size out of range: {0}")]
    Forest(#[from] miden_protocol::utils::serde::DeserializationError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::MmrDelta> for miden_protocol::crypto::merkle::mmr::MmrDelta {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::MmrDelta) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedSmtLeaf as SmtLeaf;

impl Verify for SmtLeaf {
    type Verified = miden_protocol::crypto::merkle::smt::SmtLeaf;
    type Error = miden_protocol::crypto::merkle::smt::SmtLeafError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use miden_protocol::crypto::merkle::smt::LeafIndex;
        use proto::primitives::smt_leaf::DecodedLeaf;
        match self.leaf {
            DecodedLeaf::EmptyLeafIndex(index) => {
                Ok(Self::Verified::new_empty(LeafIndex::new_max_depth(index)))
            },
            DecodedLeaf::Single(entry) => Ok(Self::Verified::new_single(entry.key, entry.value)),
            DecodedLeaf::Multiple(entries) => {
                Self::Verified::new_multiple(entries.verify().expect("infallible leaf entries"))
            },
        }
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::SmtLeaf> for miden_protocol::crypto::merkle::smt::SmtLeaf {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::SmtLeaf) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedIndexedSmtLeaf as IndexedSmtLeaf;

impl Verify for IndexedSmtLeaf {
    type Verified = (u64, miden_protocol::crypto::merkle::smt::SmtLeaf);
    type Error = miden_protocol::crypto::merkle::smt::SmtLeafError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.index, self.leaf.verify()?))
    }
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::IndexedSmtLeaf>
    for (u64, miden_protocol::crypto::merkle::smt::SmtLeaf)
{
    type Error = ConversionError;
    fn try_from(value: proto::primitives::IndexedSmtLeaf) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedSmtOpening as SmtOpening;

impl Verify for SmtOpening {
    type Verified = miden_protocol::crypto::merkle::smt::SmtProof;
    type Error = SmtOpeningError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.path.verify()?, self.leaf.verify()?)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SmtOpeningError {
    #[error(transparent)]
    Path(#[from] miden_protocol::crypto::merkle::MerkleError),
    #[error(transparent)]
    Leaf(#[from] miden_protocol::crypto::merkle::smt::SmtLeafError),
    #[error(transparent)]
    Proof(#[from] miden_protocol::crypto::merkle::smt::SmtProofError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::SmtOpening> for miden_protocol::crypto::merkle::smt::SmtProof {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::SmtOpening) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

pub use proto::primitives::DecodedPartialSmt as PartialSmt;

impl Verify for PartialSmt {
    type Verified = miden_protocol::crypto::merkle::smt::PartialSmt;
    type Error = PartialSmtError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::from_unique_nodes(self.into_unique_nodes()?)?)
    }
}

impl PartialSmt {
    /// Checks duplicate indices and constructs reconstruction input, without checking its root.
    pub fn into_unique_nodes(
        self,
    ) -> Result<miden_protocol::crypto::merkle::smt::UniqueNodes, PartialSmtError> {
        use alloc::collections::{BTreeMap, BTreeSet};

        use miden_protocol::crypto::merkle::NodeIndex;
        use miden_protocol::crypto::merkle::smt::{SMT_DEPTH, UniqueNodes};
        let mut depths = BTreeSet::new();
        let mut nodes = BTreeMap::new();
        for level in self.node_levels {
            let depth = u8::try_from(level.depth)?;
            if depth == 0 || depth >= SMT_DEPTH {
                return Err(PartialSmtError::Depth(depth));
            }
            if !depths.insert(depth) {
                return Err(PartialSmtError::DuplicateDepth(depth));
            }
            for node in level.nodes {
                let index = NodeIndex::new(depth, node.index)?;
                if nodes.insert(index, node.digest).is_some() {
                    return Err(PartialSmtError::DuplicateNode { index: node.index, depth });
                }
            }
        }
        let mut leaves = BTreeMap::new();
        for indexed in self.leaves {
            let (index, leaf) = indexed.verify()?;
            if leaves.insert(index, leaf).is_some() {
                return Err(PartialSmtError::DuplicateLeaf(index));
            }
        }
        let mut value_only_leaves = BTreeMap::new();
        for indexed in self.value_only_leaves {
            if leaves.contains_key(&indexed.index) {
                return Err(PartialSmtError::OverlappingLeaf(indexed.index));
            }
            if value_only_leaves.insert(indexed.index, indexed.value).is_some() {
                return Err(PartialSmtError::DuplicateValueOnlyLeaf(indexed.index));
            }
        }
        Ok(UniqueNodes {
            root: self.root,
            nodes,
            leaves,
            value_only_leaves,
        })
    }
}
#[derive(Debug, thiserror::Error)]
pub enum PartialSmtError {
    #[error(transparent)]
    DepthOverflow(#[from] core::num::TryFromIntError),
    #[error("partial SMT node depth {0} must be in the range 1..64")]
    Depth(u8),
    #[error("partial SMT contains duplicate node depth {0}")]
    DuplicateDepth(u8),
    #[error("partial SMT contains duplicate node index {index} at depth {depth}")]
    DuplicateNode { index: u64, depth: u8 },
    #[error("partial SMT contains duplicate leaf index {0}")]
    DuplicateLeaf(u64),
    #[error("partial SMT contains duplicate value-only leaf index {0}")]
    DuplicateValueOnlyLeaf(u64),
    #[error("partial SMT leaf index {0} has both a leaf and a value-only leaf")]
    OverlappingLeaf(u64),
    #[error(transparent)]
    Index(#[from] miden_protocol::crypto::merkle::MerkleError),
    #[error(transparent)]
    Leaf(#[from] miden_protocol::crypto::merkle::smt::SmtLeafError),
    #[error("failed to deserialize PartialSmt: {0}")]
    Reconstruction(#[from] miden_protocol::utils::serde::DeserializationError),
}

// Compatibility bridge for callers using the combined conversion API.
impl TryFrom<proto::primitives::PartialSmt> for miden_protocol::crypto::merkle::smt::PartialSmt {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::PartialSmt) -> Result<Self, Self::Error> {
        value.decode_fields()?.verify().map_err(ConversionError::new)
    }
}

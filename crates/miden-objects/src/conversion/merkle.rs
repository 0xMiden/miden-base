use alloc::vec::Vec;

use miden_protocol::Word;
use miden_protocol::crypto::merkle::mmr::MmrDelta;
use miden_protocol::crypto::merkle::smt::{PartialSmt, SmtLeaf, SmtProof, UniqueNodes};
use miden_protocol::crypto::merkle::{MerklePath, SparseMerklePath};

use crate::{ConversionError, proto};

// MERKLE PATH
// ================================================================================================

impl From<&MerklePath> for proto::primitives::MerklePath {
    fn from(value: &MerklePath) -> Self {
        let siblings = value.nodes().iter().map(Into::into).collect();
        proto::primitives::MerklePath { siblings }
    }
}

impl From<MerklePath> for proto::primitives::MerklePath {
    fn from(value: MerklePath) -> Self {
        (&value).into()
    }
}

impl TryFrom<&proto::primitives::MerklePath> for MerklePath {
    type Error = ConversionError;
    fn try_from(value: &proto::primitives::MerklePath) -> Result<Self, Self::Error> {
        value.clone().try_into()
    }
}

// SPARSE MERKLE PATH
// ================================================================================================

impl From<SparseMerklePath> for proto::primitives::SparseMerklePath {
    fn from(value: SparseMerklePath) -> Self {
        let (empty_nodes_mask, siblings) = value.into_parts();
        proto::primitives::SparseMerklePath {
            empty_nodes_mask,
            siblings: siblings.into_iter().map(Into::into).collect(),
        }
    }
}

// MMR DELTA
// ================================================================================================

impl From<MmrDelta> for proto::primitives::MmrDelta {
    fn from(value: MmrDelta) -> Self {
        let update_data = value.data.into_iter().map(Into::into).collect();
        proto::primitives::MmrDelta {
            forest: value.forest.num_leaves() as u64,
            update_data,
        }
    }
}

// SPARSE MERKLE TREE
// ================================================================================================

// SMT LEAF
// ------------------------------------------------------------------------------------------------

impl From<SmtLeaf> for proto::primitives::SmtLeaf {
    fn from(smt_leaf: SmtLeaf) -> Self {
        use proto::primitives::smt_leaf::Leaf;

        let leaf = match smt_leaf {
            SmtLeaf::Empty(leaf_index) => Leaf::EmptyLeafIndex(leaf_index.position()),
            SmtLeaf::Single(entry) => Leaf::Single(entry.into()),
            SmtLeaf::Multiple(entries) => Leaf::Multiple(proto::primitives::SmtLeafEntryList {
                entries: entries.into_iter().map(Into::into).collect(),
            }),
        };

        Self { leaf: Some(leaf) }
    }
}

// SMT LEAF ENTRY
// ------------------------------------------------------------------------------------------------

impl From<(Word, Word)> for proto::primitives::SmtLeafEntry {
    fn from((key, value): (Word, Word)) -> Self {
        Self {
            key: Some(key.into()),
            value: Some(value.into()),
        }
    }
}

// SMT PROOF
// ------------------------------------------------------------------------------------------------

impl From<SmtProof> for proto::primitives::SmtOpening {
    fn from(proof: SmtProof) -> Self {
        let (path, leaf) = proof.into_parts();
        Self {
            path: Some(path.into()),
            leaf: Some(leaf.into()),
        }
    }
}

// PARTIAL SMT
// ------------------------------------------------------------------------------------------------

impl From<UniqueNodes> for proto::primitives::PartialSmt {
    fn from(unique_nodes: UniqueNodes) -> Self {
        let UniqueNodes { root, nodes, leaves, value_only_leaves } = unique_nodes;

        let mut node_levels = Vec::new();
        let mut nodes = nodes.into_iter().peekable();
        while let Some((index, _)) = nodes.peek() {
            let depth = index.depth();
            let mut level_nodes = Vec::new();
            while let Some((index, digest)) = nodes.next_if(|(index, _)| index.depth() == depth) {
                level_nodes.push(proto::primitives::PartialSmtNode {
                    index: index.position(),
                    digest: Some(digest.into()),
                });
            }
            node_levels.push(proto::primitives::PartialSmtNodeLevel {
                depth: u32::from(depth),
                nodes: level_nodes,
            });
        }
        let leaves = leaves
            .into_iter()
            .map(|(index, leaf)| proto::primitives::IndexedSmtLeaf {
                index,
                leaf: Some(leaf.into()),
            })
            .collect();

        let value_only_leaves = value_only_leaves
            .into_iter()
            .map(|(index, value)| proto::primitives::IndexedDigest {
                index,
                value: Some(value.into()),
            })
            .collect();

        Self {
            root: Some(root.into()),
            node_levels,
            leaves,
            value_only_leaves,
        }
    }
}

impl From<PartialSmt> for proto::primitives::PartialSmt {
    fn from(partial_smt: PartialSmt) -> Self {
        partial_smt.to_unique_nodes().into()
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;
    use alloc::string::ToString;
    use alloc::vec;

    use miden_protocol::crypto::merkle::NodeIndex;
    use miden_protocol::crypto::merkle::smt::{LeafIndex, PartialSmt, SMT_DEPTH, Smt, UniqueNodes};
    use prost::Message;

    use super::*;

    #[test]
    fn partial_smt_round_trip() {
        let key0 = Word::from([1, 2, 3, 4u32]);
        let key1 = Word::from([5, 6, 7, 8u32]);
        let missing_key = Word::from([9, 10, 11, 12u32]);
        let value0 = Word::from([13, 14, 15, 16u32]);
        let value1 = Word::from([17, 18, 19, 20u32]);
        let smt = Smt::with_entries([(key0, value0), (key1, value1)]).unwrap();
        let partial_smt =
            PartialSmt::from_proofs([smt.open(&key0), smt.open(&missing_key)]).unwrap();

        let encoded: proto::primitives::PartialSmt = partial_smt.clone().into();
        assert!(encoded.node_levels.is_sorted_by_key(|level| level.depth));

        let decoded = PartialSmt::try_from(encoded).unwrap();

        assert_eq!(decoded, partial_smt);
        assert_eq!(decoded.get_value(&key0).unwrap(), value0);
        assert_eq!(decoded.get_value(&missing_key).unwrap(), Word::empty());
    }

    #[test]
    fn partial_smt_encoding_is_canonical_for_equivalent_unique_nodes() {
        let mut first = UniqueNodes::empty();
        first.nodes.insert(NodeIndex::new(1, 1).unwrap(), Word::from([1, 2, 3, 4u32]));
        first
            .nodes
            .insert(NodeIndex::new(1, 0).unwrap(), Word::from([9, 10, 11, 12u32]));
        first.leaves = BTreeMap::from([
            (2, SmtLeaf::new_empty(LeafIndex::new_max_depth(2))),
            (1, SmtLeaf::new_empty(LeafIndex::new_max_depth(1))),
        ]);
        first.value_only_leaves =
            BTreeMap::from([(2, Word::from([5, 6, 7, 8u32])), (1, Word::from([9, 10, 11, 12u32]))]);

        let mut second = first.clone();
        second.nodes = BTreeMap::from([
            (NodeIndex::new(1, 0).unwrap(), Word::from([9, 10, 11, 12u32])),
            (NodeIndex::new(1, 1).unwrap(), Word::from([1, 2, 3, 4u32])),
        ]);
        second.leaves = BTreeMap::from([
            (1, SmtLeaf::new_empty(LeafIndex::new_max_depth(1))),
            (2, SmtLeaf::new_empty(LeafIndex::new_max_depth(2))),
        ]);
        second.value_only_leaves =
            BTreeMap::from([(1, Word::from([9, 10, 11, 12u32])), (2, Word::from([5, 6, 7, 8u32]))]);

        let first: proto::primitives::PartialSmt = first.into();
        let second: proto::primitives::PartialSmt = second.into();

        assert_eq!(first, second);
        assert_eq!(first.encode_to_vec(), second.encode_to_vec());
    }

    #[test]
    fn partial_smt_encoding_preserves_nodes_at_every_depth() {
        let expected_nodes = BTreeMap::from([
            (NodeIndex::new(1, 0).unwrap(), Word::from([1, 2, 3, 4u32])),
            (NodeIndex::new(1, 1).unwrap(), Word::from([5, 6, 7, 8u32])),
            (NodeIndex::new(2, 0).unwrap(), Word::from([9, 10, 11, 12u32])),
            (NodeIndex::new(2, 3).unwrap(), Word::from([13, 14, 15, 16u32])),
            (NodeIndex::new(3, 5).unwrap(), Word::from([17, 18, 19, 20u32])),
        ]);
        let mut unique_nodes = UniqueNodes::empty();
        unique_nodes.nodes = expected_nodes.clone();

        let encoded: proto::primitives::PartialSmt = unique_nodes.into();

        assert_eq!(
            encoded.node_levels.iter().map(|level| level.depth).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let decoded = UniqueNodes::try_from(encoded).unwrap();
        assert_eq!(decoded.nodes, expected_nodes);
    }

    fn empty_partial_smt_message() -> proto::primitives::PartialSmt {
        proto::primitives::PartialSmt {
            root: Some(PartialSmt::EMPTY_ROOT.into()),
            node_levels: vec![],
            leaves: vec![],
            value_only_leaves: vec![],
        }
    }

    fn assert_partial_smt_decode_error(
        encoded: proto::primitives::PartialSmt,
        expected_error: &str,
    ) {
        let error = PartialSmt::try_from(encoded).unwrap_err();
        assert_eq!(error.to_string(), expected_error);
    }

    #[test]
    fn partial_smt_rejects_missing_root() {
        let mut encoded = empty_partial_smt_message();
        encoded.root = None;
        assert_partial_smt_decode_error(
            encoded,
            "root: field miden_objects::proto::primitives::PartialSmt::root is missing",
        );
    }

    #[test]
    fn partial_smt_rejects_duplicate_depth() {
        let mut encoded = empty_partial_smt_message();
        encoded.node_levels = vec![
            proto::primitives::PartialSmtNodeLevel { depth: 1, nodes: vec![] },
            proto::primitives::PartialSmtNodeLevel { depth: 1, nodes: vec![] },
        ];
        assert_partial_smt_decode_error(encoded, "partial SMT contains duplicate node depth 1");
    }

    #[test]
    fn partial_smt_rejects_invalid_node_index() {
        let mut encoded = empty_partial_smt_message();
        encoded.node_levels = vec![proto::primitives::PartialSmtNodeLevel {
            depth: 1,
            nodes: vec![proto::primitives::PartialSmtNode {
                index: 2,
                digest: Some(Word::empty().into()),
            }],
        }];
        assert_partial_smt_decode_error(encoded, "node index position 2 is not valid for depth 1");
    }

    #[test]
    fn partial_smt_rejects_missing_node_digest() {
        let mut encoded = empty_partial_smt_message();
        encoded.node_levels = vec![proto::primitives::PartialSmtNodeLevel {
            depth: 1,
            nodes: vec![proto::primitives::PartialSmtNode { index: 0, digest: None }],
        }];
        assert_partial_smt_decode_error(
            encoded,
            "node_levels[0].nodes[0].digest: field miden_objects::proto::primitives::PartialSmtNode::digest is missing",
        );
    }

    #[test]
    fn partial_smt_rejects_missing_leaf() {
        let mut encoded = empty_partial_smt_message();
        encoded.leaves = vec![proto::primitives::IndexedSmtLeaf { index: 0, leaf: None }];
        assert_partial_smt_decode_error(
            encoded,
            "leaves[0].leaf: field miden_objects::proto::primitives::IndexedSmtLeaf::leaf is missing",
        );
    }

    #[test]
    fn partial_smt_rejects_missing_value_only_leaf() {
        let mut encoded = empty_partial_smt_message();
        encoded.value_only_leaves =
            vec![proto::primitives::IndexedDigest { index: 0, value: None }];
        assert_partial_smt_decode_error(
            encoded,
            "value_only_leaves[0].value: field miden_objects::proto::primitives::IndexedDigest::value is missing",
        );
    }

    #[test]
    fn partial_smt_rejects_depth_overflow() {
        let mut encoded = empty_partial_smt_message();
        encoded.node_levels =
            vec![proto::primitives::PartialSmtNodeLevel { depth: 256, nodes: vec![] }];
        assert_partial_smt_decode_error(encoded, "out of range integral type conversion attempted");
    }

    #[test]
    fn partial_smt_rejects_zero_depth() {
        let mut encoded = empty_partial_smt_message();
        encoded.node_levels =
            vec![proto::primitives::PartialSmtNodeLevel { depth: 0, nodes: vec![] }];
        assert_partial_smt_decode_error(
            encoded,
            "partial SMT node depth 0 must be in the range 1..64",
        );
    }

    #[test]
    fn partial_smt_rejects_smt_depth() {
        let mut encoded = empty_partial_smt_message();
        encoded.node_levels = vec![proto::primitives::PartialSmtNodeLevel {
            depth: u32::from(SMT_DEPTH),
            nodes: vec![],
        }];
        assert_partial_smt_decode_error(
            encoded,
            "partial SMT node depth 64 must be in the range 1..64",
        );
    }

    #[test]
    fn partial_smt_rejects_duplicate_node_index() {
        let mut encoded = empty_partial_smt_message();
        encoded.node_levels = vec![proto::primitives::PartialSmtNodeLevel {
            depth: 1,
            nodes: vec![
                proto::primitives::PartialSmtNode {
                    index: 0,
                    digest: Some(Word::empty().into()),
                },
                proto::primitives::PartialSmtNode {
                    index: 0,
                    digest: Some(Word::empty().into()),
                },
            ],
        }];
        assert_partial_smt_decode_error(
            encoded,
            "partial SMT contains duplicate node index 0 at depth 1",
        );
    }

    #[test]
    fn partial_smt_rejects_duplicate_leaf_index() {
        let mut encoded = empty_partial_smt_message();
        encoded.leaves = vec![
            proto::primitives::IndexedSmtLeaf {
                index: 0,
                leaf: Some(SmtLeaf::new_empty(LeafIndex::new_max_depth(0)).into()),
            },
            proto::primitives::IndexedSmtLeaf {
                index: 0,
                leaf: Some(SmtLeaf::new_empty(LeafIndex::new_max_depth(0)).into()),
            },
        ];
        assert_partial_smt_decode_error(encoded, "partial SMT contains duplicate leaf index 0");
    }

    #[test]
    fn partial_smt_rejects_duplicate_value_only_leaf_index() {
        let mut encoded = empty_partial_smt_message();
        encoded.value_only_leaves = vec![
            proto::primitives::IndexedDigest {
                index: 0,
                value: Some(Word::empty().into()),
            },
            proto::primitives::IndexedDigest {
                index: 0,
                value: Some(Word::empty().into()),
            },
        ];
        assert_partial_smt_decode_error(
            encoded,
            "partial SMT contains duplicate value-only leaf index 0",
        );
    }

    #[test]
    fn partial_smt_rejects_overlapping_leaf_index() {
        let mut encoded = empty_partial_smt_message();
        encoded.leaves = vec![proto::primitives::IndexedSmtLeaf {
            index: 0,
            leaf: Some(SmtLeaf::new_empty(LeafIndex::new_max_depth(0)).into()),
        }];
        encoded.value_only_leaves = vec![proto::primitives::IndexedDigest {
            index: 0,
            value: Some(Word::empty().into()),
        }];
        assert_partial_smt_decode_error(
            encoded,
            "partial SMT leaf index 0 has both a leaf and a value-only leaf",
        );
    }

    #[test]
    fn partial_smt_rejects_embedded_leaf_index_mismatch() {
        let mut encoded = empty_partial_smt_message();
        encoded.leaves = vec![proto::primitives::IndexedSmtLeaf {
            index: 0,
            leaf: Some(SmtLeaf::new_empty(LeafIndex::new_max_depth(1)).into()),
        }];
        assert_partial_smt_decode_error(
            encoded,
            "failed to deserialize PartialSmt: invalid value: Node index 0 did not match the embedded leaf index 1",
        );
    }

    #[test]
    fn partial_smt_rejects_reconstruction_missing_node() {
        let mut encoded = empty_partial_smt_message();
        encoded.node_levels = vec![proto::primitives::PartialSmtNodeLevel {
            depth: 1,
            nodes: vec![proto::primitives::PartialSmtNode {
                index: 0,
                digest: Some(Word::empty().into()),
            }],
        }];
        assert_partial_smt_decode_error(
            encoded,
            "failed to deserialize PartialSmt: invalid value: inner node hash is inconsistent with parent",
        );
    }
}

impl TryFrom<proto::primitives::PartialSmt> for UniqueNodes {
    type Error = ConversionError;
    fn try_from(value: proto::primitives::PartialSmt) -> Result<Self, Self::Error> {
        use crate::DecodeMessage;
        value.decode_fields()?.into_unique_nodes().map_err(ConversionError::new)
    }
}

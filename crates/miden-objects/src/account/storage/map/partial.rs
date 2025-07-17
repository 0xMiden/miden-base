use miden_crypto::{
    Word,
    merkle::{InnerNodeInfo, LeafIndex, MerkleError, PartialSmt, SMT_DEPTH, SmtLeaf, SmtProof},
};
use vm_core::utils::{Deserializable, Serializable};

use crate::account::StorageMap;

/// A lightweight snapshot of a full [`StorageMap`].
///
/// A partial storage map carries only the Merkle authentication data a transaction will need.
/// Every included entry pairs a value with its proof, letting the transaction kernel verify reads
/// (and prepare writes) without needing the complete tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialStorageMap {
    storage_smt: PartialSmt,
}

impl PartialStorageMap {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Instantiates a new [`PartialStorageMap`] by calling [`PartialSmt::add_path`] for all
    /// [`SmtProof`]s in the provided iterator.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the new root after the insertion of a (leaf, path) tuple does not match the existing root
    ///   (except if the tree was previously empty).
    pub fn from_proofs<I>(proofs: I) -> Result<Self, MerkleError>
    where
        I: IntoIterator<Item = SmtProof>,
    {
        let mut partial_smt = PartialSmt::new();

        for (proof, leaf) in proofs.into_iter().map(SmtProof::into_parts) {
            partial_smt.add_path(leaf, proof)?;
        }

        Ok(PartialStorageMap { storage_smt: partial_smt })
    }

    pub fn partial_smt(&self) -> &PartialSmt {
        &self.storage_smt
    }

    pub fn root(&self) -> Word {
        self.storage_smt.root()
    }

    // ITERATORS
    // --------------------------------------------------------------------------------------------

    /// Returns an iterator over the leaves of the underlying [`PartialSmt`].
    pub fn leaves(&self) -> impl Iterator<Item = (LeafIndex<SMT_DEPTH>, &SmtLeaf)> {
        self.storage_smt.leaves()
    }

    /// Returns an iterator over the key value pairs of the map.
    pub fn entries(&self) -> impl Iterator<Item = (Word, Word)> {
        self.storage_smt.entries().copied()
    }

    /// Returns an iterator over the inner nodes of the underlying [`PartialSmt`].
    pub fn inner_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> + '_ {
        self.storage_smt.inner_nodes()
    }
}

impl From<StorageMap> for PartialStorageMap {
    fn from(value: StorageMap) -> Self {
        let v = value.smt;

        PartialStorageMap { storage_smt: v.into() }
    }
}

impl Serializable for PartialStorageMap {
    fn write_into<W: vm_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(&self.storage_smt);
    }
}

impl Deserializable for PartialStorageMap {
    fn read_from<R: vm_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, vm_processor::DeserializationError> {
        let storage: PartialSmt = source.read()?;
        Ok(PartialStorageMap { storage_smt: storage })
    }
}

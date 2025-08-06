use miden_crypto::Word;
use miden_crypto::merkle::{
    InnerNodeInfo,
    LeafIndex,
    MerkleError,
    PartialSmt,
    SMT_DEPTH,
    SmtLeaf,
    SmtProof,
};
use vm_core::utils::{Deserializable, Serializable};

use crate::account::StorageMap;

/// A partial representation of a [`StorageMap`], containing only proofs for a subset of the
/// key-value pairs.
///
/// A partial storage map carries only the Merkle authentication data a transaction will need.
/// Every included entry pairs a value with its proof, letting the transaction kernel verify reads
/// (and prepare writes) without needing the complete tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialStorageMap {
    partial_smt: PartialSmt,
}

impl PartialStorageMap {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns a new instance of partial storage map with the specified partial SMT.
    pub fn new(partial_smt: PartialSmt) -> Self {
        PartialStorageMap { partial_smt }
    }

    pub fn partial_smt(&self) -> &PartialSmt {
        &self.partial_smt
    }

    pub fn root(&self) -> Word {
        self.partial_smt.root()
    }

    /// Returns an opening of the leaf associated with `key`.
    ///
    /// Conceptually, an opening is a Merkle path to the leaf, as well as the leaf itself.
    /// The key needs to be hashed to have a behavior in line with [`StorageMap`]. For more details
    /// as to why this is needed, refer to the docs for that struct.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the key is not tracked by this partial storage map.
    pub fn open(&self, key: &Word) -> Result<SmtProof, MerkleError> {
        let key = StorageMap::hash_key(*key);
        self.partial_smt.open(&key)
    }

    // ITERATORS
    // --------------------------------------------------------------------------------------------

    /// Returns an iterator over the leaves of the underlying [`PartialSmt`].
    pub fn leaves(&self) -> impl Iterator<Item = (LeafIndex<SMT_DEPTH>, &SmtLeaf)> {
        self.partial_smt.leaves()
    }

    /// Returns an iterator over the key value pairs of the map.
    pub fn entries(&self) -> impl Iterator<Item = (Word, Word)> {
        self.partial_smt.entries().copied()
    }

    /// Returns an iterator over the inner nodes of the underlying [`PartialSmt`].
    pub fn inner_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> + '_ {
        self.partial_smt.inner_nodes()
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Adds an [`SmtProof`] to this [`PartialStorageMap`].
    pub fn add(&mut self, proof: SmtProof) -> Result<(), MerkleError> {
        self.partial_smt.add_proof(proof)
    }
}

impl From<StorageMap> for PartialStorageMap {
    fn from(value: StorageMap) -> Self {
        let v = value.smt;

        PartialStorageMap { partial_smt: v.into() }
    }
}

impl From<PartialSmt> for PartialStorageMap {
    fn from(partial_smt: PartialSmt) -> Self {
        PartialStorageMap { partial_smt }
    }
}

impl Serializable for PartialStorageMap {
    fn write_into<W: vm_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(&self.partial_smt);
    }
}

impl Deserializable for PartialStorageMap {
    fn read_from<R: vm_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, vm_processor::DeserializationError> {
        let storage: PartialSmt = source.read()?;
        Ok(PartialStorageMap { partial_smt: storage })
    }
}

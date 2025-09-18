use alloc::collections::BTreeMap;

use miden_core::utils::{Deserializable, Serializable};
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

use crate::account::{StorageMap, StorageMapWitness};

/// A partial representation of a [`StorageMap`], containing only proofs for a subset of the
/// key-value pairs.
///
/// A partial storage map carries only the Merkle authentication data a transaction will need.
/// Every included entry pairs a value with its proof, letting the transaction kernel verify reads
/// (and prepare writes) without needing the complete tree.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PartialStorageMap {
    partial_smt: PartialSmt,
    map: BTreeMap<Word, Word>,
}

impl PartialStorageMap {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Returns a new instance of partial storage map with the specified partial SMT and stored
    /// entries.
    pub fn from_witnesses(
        witnesses: impl IntoIterator<Item = StorageMapWitness>,
    ) -> Result<Self, MerkleError> {
        let mut partial_smt = PartialSmt::default();
        let mut map = BTreeMap::new();

        for witness in witnesses.into_iter() {
            map.extend(witness.entries());
            let smt_proof = SmtProof::from(witness);
            partial_smt.add_proof(smt_proof)?;
        }

        Ok(PartialStorageMap { partial_smt, map })
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
    pub fn entries(&self) -> impl Iterator<Item = (&Word, &Word)> {
        self.map.iter()
    }

    /// Returns an iterator over the inner nodes of the underlying [`PartialSmt`].
    pub fn inner_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> + '_ {
        self.partial_smt.inner_nodes()
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Adds a [`StorageMapWitness`] for the specific key-value pair to this [`PartialStorageMap`].
    pub fn add(&mut self, witness: StorageMapWitness) -> Result<(), MerkleError> {
        self.map.extend(witness.entries().map(|(key, value)| (*key, *value)));
        self.partial_smt.add_proof(SmtProof::from(witness))
    }
}

impl From<StorageMap> for PartialStorageMap {
    fn from(value: StorageMap) -> Self {
        let smt = value.smt;
        let map = value.map;

        PartialStorageMap { partial_smt: smt.into(), map }
    }
}

impl Serializable for PartialStorageMap {
    fn write_into<W: miden_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(&self.partial_smt);
        target.write_many(self.map.iter());
    }
}

impl Deserializable for PartialStorageMap {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, miden_processor::DeserializationError> {
        let storage: PartialSmt = source.read()?;
        let map_keys: BTreeMap<Word, Word> = source.read()?;

        // TODO: Validate.

        Ok(PartialStorageMap { partial_smt: storage, map: map_keys })
    }
}

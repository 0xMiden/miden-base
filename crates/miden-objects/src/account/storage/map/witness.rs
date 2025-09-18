use alloc::collections::BTreeMap;

use miden_crypto::merkle::{InnerNodeInfo, SmtProof};

use crate::Word;
use crate::account::StorageMap;
use crate::errors::StorageMapError;

/// A witness of an asset in a [`StorageMap`](super::StorageMap).
///
/// It proves inclusion of a certain storage item in the map.
///
/// TODO: Add guarantees.
/// TODO: Add limitations of map_keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageMapWitness {
    proof: SmtProof,
    map: BTreeMap<Word, Word>,
}

impl StorageMapWitness {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`StorageMapWitness`] from an SMT proof.
    pub fn new(
        proof: SmtProof,
        map_keys: impl IntoIterator<Item = Word>,
    ) -> Result<Self, StorageMapError> {
        let mut map = BTreeMap::new();

        for map_key in map_keys.into_iter() {
            let hashed_map_key = StorageMap::hash_key(map_key);
            let value =
                proof.get(&hashed_map_key).ok_or(StorageMapError::MissingKey { map_key })?;
            map.insert(map_key, value);
        }

        Ok(Self { proof, map })
    }

    /// Creates a new [`StorageMapWitness`] from an SMT proof and a set of key value pairs.
    ///
    /// # Warning
    ///
    /// This does not validate any of the guarantees of this type. See the type-level docs for more
    /// details.
    pub fn new_unchecked(
        proof: SmtProof,
        key_values: impl IntoIterator<Item = (Word, Word)>,
    ) -> Self {
        Self {
            proof,
            map: key_values.into_iter().collect(),
        }
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the underlying [`SmtProof`].
    pub fn as_proof(&self) -> &SmtProof {
        &self.proof
    }

    /// Searches for a value in the witness with the given unhashed `map_key`.
    pub fn find(&self, map_key: Word) -> Option<Word> {
        let hashed_map_key = StorageMap::hash_key(map_key);
        self.hashed_entries()
            .find_map(|(key, value)| if *key == hashed_map_key { Some(*value) } else { None })
    }

    /// TODO
    pub fn entries(&self) -> impl Iterator<Item = (&Word, &Word)> {
        self.map.iter()
    }

    /// Returns an iterator over the key-value pairs in this witness.
    ///
    /// Note that the returned key is the hashed map key.
    pub fn hashed_entries(&self) -> impl Iterator<Item = (&Word, &Word)> {
        // Convert &(Word, Word) into (&Word, &Word) as it is more flexible.
        self.proof.leaf().entries().into_iter().map(|(key, value)| (key, value))
    }

    /// Returns an iterator over every inner node of this witness' merkle path.
    pub fn authenticated_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> + '_ {
        self.proof
            .path()
            .authenticated_nodes(self.proof.leaf().index().value(), self.proof.leaf().hash())
            .expect("leaf index is u64 and should be less than 2^SMT_DEPTH")
    }
}

impl From<StorageMapWitness> for SmtProof {
    fn from(witness: StorageMapWitness) -> Self {
        witness.proof
    }
}

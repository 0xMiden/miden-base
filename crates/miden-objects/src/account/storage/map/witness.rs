use miden_crypto::merkle::{InnerNodeInfo, SmtProof};

use crate::Word;

/// A witness of an asset in a [`StorageMap`](super::StorageMap).
///
/// It proves inclusion of a certain storage item in the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageMapWitness(SmtProof);

impl StorageMapWitness {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`StorageMapWitness`] from an SMT proof.
    pub fn new(smt_proof: SmtProof) -> Self {
        Self(smt_proof)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Searches for a value in the witness with the given `map_key`.
    pub fn find(&self, hashed_map_key: Word) -> Option<Word> {
        self.entries()
            .find_map(|(key, value)| if *key == hashed_map_key { Some(*value) } else { None })
    }

    /// Returns an iterator over the key-value pairs in this witness.
    ///
    /// Note that the returned key is the hashed map key.
    pub fn entries(&self) -> impl Iterator<Item = (&Word, &Word)> {
        // Convert &(Word, Word) into (&Word, &Word) as it is more flexible.
        self.0.leaf().entries().into_iter().map(|(key, value)| (key, value))
    }

    /// Returns an iterator over every inner node of this witness' merkle path.
    pub fn authenticated_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> + '_ {
        self.0
            .path()
            .authenticated_nodes(self.0.leaf().index().value(), self.0.leaf().hash())
            .expect("leaf index is u64 and should be less than 2^SMT_DEPTH")
    }
}

impl From<StorageMapWitness> for SmtProof {
    fn from(witness: StorageMapWitness) -> Self {
        witness.0
    }
}

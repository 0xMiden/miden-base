use alloc::collections::BTreeMap;

use miden_crypto::merkle::{InnerNodeInfo, SmtProof};

use crate::Word;
use crate::account::StorageMap;
use crate::errors::StorageMapError;

/// A witness of an asset in a [`StorageMap`](super::StorageMap).
///
/// It proves inclusion of a certain storage item in the map.
///
/// ## Guarantees
///
/// This type guarantees that the raw key-value pairs it contains are all present in the
/// contained SMT proof. Note that the inverse is not necessarily true. The proof may contain more
/// entries than the map because to prove inclusion of a given raw key A an
/// [`SmtLeaf::Multiple`](miden_crypto::merkle::SmtLeaf::Multiple) may be present that contains both
/// keys hash(A) and hash(B). However, B may not be present in the key-value pairs and this is a
/// valid state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageMapWitness {
    proof: SmtProof,
    /// The entries of the map where the key is the raw user-chosen one.
    ///
    /// It is an invariant of this type that the map's entries are always consistent with the SMT's
    /// entries and vice-versa.
    entries: BTreeMap<Word, Word>,
}

impl StorageMapWitness {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`StorageMapWitness`] from an SMT proof and a provided set of map keys.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Any of the map keys is not contained in the proof.
    pub fn new(
        proof: SmtProof,
        raw_keys: impl IntoIterator<Item = Word>,
    ) -> Result<Self, StorageMapError> {
        let mut entries = BTreeMap::new();

        for raw_key in raw_keys.into_iter() {
            let hashed_map_key = StorageMap::hash_key(raw_key);
            let value =
                proof.get(&hashed_map_key).ok_or(StorageMapError::MissingKey { raw_key })?;
            entries.insert(raw_key, value);
        }

        Ok(Self { proof, entries })
    }

    /// Creates a new [`StorageMapWitness`] from an SMT proof and a set of raw key value pairs.
    ///
    /// # Warning
    ///
    /// This does not validate any of the guarantees of this type. See the type-level docs for more
    /// details.
    pub fn new_unchecked(
        proof: SmtProof,
        raw_key_values: impl IntoIterator<Item = (Word, Word)>,
    ) -> Self {
        Self {
            proof,
            entries: raw_key_values.into_iter().collect(),
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

    /// Returns an iterator over the key-value pairs in this witness.
    ///
    /// Note that the returned key is the raw map key.
    pub fn entries(&self) -> impl Iterator<Item = (&Word, &Word)> {
        self.entries.iter()
    }

    /// Returns an iterator over the key-value pairs in this witness.
    ///
    /// Note that the returned key is the hashed map key.
    pub(crate) fn hashed_entries(&self) -> impl Iterator<Item = (&Word, &Word)> {
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

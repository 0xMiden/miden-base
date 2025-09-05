use miden_crypto::merkle::{InnerNodeInfo, SmtProof};

/// A witness of an asset in a [`StorageMap`](super::StorageMap).
///
/// It proves inclusion of a certain storage item in the map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageMapWitness(SmtProof);

impl StorageMapWitness {
    /// Creates a new [`StorageMapWitness`] from an SMT proof.
    pub fn new(smt_proof: SmtProof) -> Self {
        Self(smt_proof)
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

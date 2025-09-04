use miden_crypto::merkle::SmtProof;

/// A witness of an asset in an [`AssetVault`](super::AssetVault).
///
/// It proves inclusion of a certain asset in the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWitness(SmtProof);

impl AssetWitness {
    /// Creates a new [`AssetWitness`] from an SMT proof without checking that the proof contains
    /// valid assets.
    pub(crate) fn new_unchecked(smt_proof: SmtProof) -> Self {
        Self(smt_proof)
    }
}

impl From<AssetWitness> for SmtProof {
    fn from(witness: AssetWitness) -> Self {
        witness.0
    }
}

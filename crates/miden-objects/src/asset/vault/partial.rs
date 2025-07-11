use alloc::vec::Vec;

use miden_crypto::merkle::{InnerNodeInfo, SmtLeaf, SmtProof};
use vm_core::utils::{Deserializable, Serializable};

use super::AssetVault;
use crate::Word;

/// A partial representation of an asset vault, containing only proofs for a subset of assets.
///
/// Partial vault is used to provide verifiable access to specific assets in a vault
/// without the need to provide the full vault data. It contains all required data for loading
/// vault data into the transaction kernel for transaction execution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialVault {
    /// Root of the asset vault tree.
    root: Word,
    /// Merkle proofs for assets in an account, typically a subset of all assets.
    vault_proofs: Vec<SmtProof>,
}

impl PartialVault {
    /// Returns a new instance of partial vault with the specified root and vault proofs.
    pub fn new(root: Word, vault_proofs: Vec<SmtProof>) -> Self {
        PartialVault { root, vault_proofs }
    }

    /// Returns the root of the partial vault.
    pub fn root(&self) -> Word {
        self.root
    }

    /// Returns an iterator over all inner nodes in the Sparse Merkle Tree proofs.
    ///
    /// This is useful for reconstructing parts of the Sparse Merkle Tree or for
    /// verification purposes.
    pub fn inner_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> + '_ {
        self.vault_proofs.iter().flat_map(|proof| {
            let leaf = proof.leaf();
            proof.path().authenticated_nodes(leaf.index().value(), leaf.hash()).unwrap()
        })
    }

    /// Returns an iterator over all leaves in the Sparse Merkle Tree proofs.
    ///
    /// Each item returned is a tuple containing the leaf index and a reference to the leaf.
    pub fn leaves(&self) -> impl Iterator<Item = &SmtLeaf> {
        self.vault_proofs.iter().map(SmtProof::leaf)
    }
}

impl From<&AssetVault> for PartialVault {
    fn from(value: &AssetVault) -> Self {
        let root = value.root();
        let vault_proofs: Vec<SmtProof> = value
            .asset_tree()
            .entries()
            .map(|(key, _)| value.asset_tree().open(key))
            .collect();

        PartialVault { root, vault_proofs }
    }
}

impl Serializable for PartialVault {
    fn write_into<W: vm_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(self.root);
        target.write(&self.vault_proofs);
    }
}

impl Deserializable for PartialVault {
    fn read_from<R: vm_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, vm_processor::DeserializationError> {
        let root = source.read()?;
        let vault_proofs = source.read()?;

        Ok(PartialVault { root, vault_proofs })
    }
}

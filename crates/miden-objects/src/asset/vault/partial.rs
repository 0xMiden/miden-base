use miden_crypto::merkle::{InnerNodeInfo, MerkleError, PartialSmt, SmtLeaf};
use vm_core::utils::{Deserializable, Serializable};

use super::AssetVault;
use crate::Word;
use crate::asset::Asset;

/// A partial representation of an [`AssetVault`], containing only proofs for a subset of assets.
///
/// Partial vault is used to provide verifiable access to specific assets in a vault
/// without the need to provide the full vault data. It contains all required data for loading
/// vault data into the transaction kernel for transaction execution.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PartialVault {
    /// An SMT with a partial view into an account's full [`AssetVault`].
    partial_smt: PartialSmt,
}

impl PartialVault {
    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the root of the partial vault.
    pub fn root(&self) -> Word {
        self.partial_smt.root()
    }

    /// Returns an iterator over all inner nodes in the Sparse Merkle Tree proofs.
    ///
    /// This is useful for reconstructing parts of the Sparse Merkle Tree or for
    /// verification purposes.
    pub fn inner_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> + '_ {
        self.partial_smt.inner_nodes()
    }

    /// Returns an iterator over all leaves in the Sparse Merkle Tree proofs.
    ///
    /// Each item returned is a tuple containing the leaf index and a reference to the leaf.
    pub fn leaves(&self) -> impl Iterator<Item = &SmtLeaf> {
        self.partial_smt.leaves().map(|(_, leaf)| leaf)
    }

    /// Returns the [`Asset`] associated with the given `vault_key`.
    ///
    /// The return value is `None` if the asset does not exist in the vault.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the key is not tracked by this partial SMT.
    pub fn get(&self, vault_key: Word) -> Result<Option<Asset>, MerkleError> {
        self.partial_smt.get_value(&vault_key).map(|word| {
            if word.is_empty() {
                None
            } else {
                // SAFETY: If this returned a non-empty word, then it should be a valid asset,
                // because the vault should only track valid ones.
                Some(Asset::try_from(word).expect("partial vault should only track valid assets"))
            }
        })
    }
}

impl From<&AssetVault> for PartialVault {
    fn from(value: &AssetVault) -> Self {
        let vault_partial_smt = value.asset_tree.clone().into();

        PartialVault { partial_smt: vault_partial_smt }
    }
}

impl Serializable for PartialVault {
    fn write_into<W: vm_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(&self.partial_smt)
    }
}

impl Deserializable for PartialVault {
    fn read_from<R: vm_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, vm_processor::DeserializationError> {
        let vault_partial_smt = source.read()?;

        Ok(PartialVault { partial_smt: vault_partial_smt })
    }
}

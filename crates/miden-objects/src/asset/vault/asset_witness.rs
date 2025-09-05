use miden_crypto::merkle::{InnerNodeInfo, SmtProof};

use crate::AssetError;
use crate::asset::Asset;

/// A witness of an asset in an [`AssetVault`](super::AssetVault).
///
/// It proves inclusion of a certain asset in the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetWitness(SmtProof);

impl AssetWitness {
    /// Creates a new [`AssetWitness`] from an SMT proof.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - any of the entries in the SMT leaf is not a valid asset.
    /// - any of the entries' vault keys does not match the expected vault key of the asset.
    pub fn new(smt_proof: SmtProof) -> Result<Self, AssetError> {
        for (vault_key, asset) in smt_proof.leaf().entries() {
            let asset = Asset::try_from(asset)?;
            if asset.vault_key() != *vault_key {
                return Err(AssetError::VaultKeyMismatch {
                    actual: *vault_key,
                    expected: asset.vault_key(),
                });
            }
        }

        Ok(Self(smt_proof))
    }

    /// Creates a new [`AssetWitness`] from an SMT proof without checking that the proof contains
    /// valid assets.
    ///
    /// Prefer [`AssetWitness::new`] whenever possible.
    pub fn new_unchecked(smt_proof: SmtProof) -> Self {
        Self(smt_proof)
    }

    /// Returns the assets in this witness.
    pub fn assets(&self) -> impl Iterator<Item = Asset> {
        // Check if we can avoid cloning the vector.
        self.0.leaf().entries().to_vec().into_iter().map(|(_key, value)| {
            Asset::try_from(value).expect("asset witness should track valid assets")
        })
    }

    /// Returns an iterator over every inner node of this witness' merkle path.
    pub fn authenticated_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> + '_ {
        self.0
            .path()
            .authenticated_nodes(self.0.leaf().index().value(), self.0.leaf().hash())
            .expect("leaf index is u64 and should be less than 2^SMT_DEPTH")
    }
}

impl From<AssetWitness> for SmtProof {
    fn from(witness: AssetWitness) -> Self {
        witness.0
    }
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use miden_crypto::merkle::Smt;

    use super::*;
    use crate::Word;
    use crate::asset::{FungibleAsset, NonFungibleAsset};

    /// Tests that constructing an asset witness fails if any asset in the smt proof is invalid.
    #[test]
    fn create_asset_witness_fails_on_invalid_asset() -> anyhow::Result<()> {
        let invalid_asset = Word::from([0, 0, 0, 5u32]);
        let smt = Smt::with_entries([(invalid_asset, invalid_asset)])?;
        let proof = smt.open(&invalid_asset);

        let err = AssetWitness::new(proof).unwrap_err();

        assert_matches!(err, AssetError::InvalidFaucetAccountId(_));

        Ok(())
    }

    /// Tests that constructing an asset witness fails if the vault key is from a fungible asset and
    /// the asset is a non-fungible one.
    #[test]
    fn create_asset_witness_fails_on_vault_key_mismatch() -> anyhow::Result<()> {
        let fungible_asset = FungibleAsset::mock(500);
        let non_fungible_asset = NonFungibleAsset::mock(&[1]);

        let smt = Smt::with_entries([(fungible_asset.vault_key(), non_fungible_asset.into())])?;
        let proof = smt.open(&fungible_asset.vault_key());

        let err = AssetWitness::new(proof).unwrap_err();

        assert_matches!(err, AssetError::VaultKeyMismatch { actual, expected } => {
            assert_eq!(actual, fungible_asset.vault_key());
            assert_eq!(expected, non_fungible_asset.vault_key());
        });

        Ok(())
    }
}

pub use proto::account::DecodedPartialStorageMap as PartialStorageMap;

use super::{AccountCodeError, StorageHeaderError};
use crate::{Verify, proto};

#[cfg(test)]
mod tests;

impl Verify for PartialStorageMap {
    type Verified = miden_protocol::account::PartialStorageMap;
    type Error = PartialStorageMapError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::try_from_parts(
            self.smt.verify()?,
            self.keys.into_iter().map(miden_protocol::account::StorageMapKey::from_raw),
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PartialStorageMapError {
    #[error("{0}")]
    Smt(#[from] crate::decoded::primitives::PartialSmtError),
    #[error("{0}")]
    Storage(#[from] miden_protocol::crypto::merkle::MerkleError),
}

pub use proto::account::DecodedPartialStorage as PartialStorage;

impl Verify for PartialStorage {
    type Verified = miden_protocol::account::PartialStorage;
    type Error = PartialStorageError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let mut roots = alloc::collections::BTreeSet::new();
        let mut maps = alloc::vec::Vec::new();
        for map in self.maps {
            let map = map.verify()?;
            if !roots.insert(map.root()) {
                return Err(PartialStorageError::DuplicateRoot(map.root()));
            }
            maps.push(map);
        }
        Ok(Self::Verified::new(self.header.verify()?, maps)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PartialStorageError {
    #[error("duplicate partial storage map root {0}")]
    DuplicateRoot(miden_protocol::Word),
    #[error("{0}")]
    Map(#[from] PartialStorageMapError),
    #[error("{0}")]
    Header(#[from] StorageHeaderError),
    #[error("{0}")]
    Storage(#[from] miden_protocol::errors::AccountError),
}

pub use proto::account::DecodedPartialVault as PartialVault;

impl Verify for PartialVault {
    type Verified = miden_protocol::asset::PartialVault;
    type Error = PartialVaultError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let ids = self
            .asset_ids
            .into_iter()
            .map(miden_protocol::asset::AssetId::try_from)
            .collect::<Result<alloc::vec::Vec<_>, _>>()?;
        Ok(Self::Verified::try_from_parts(self.smt.verify()?, ids)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PartialVaultError {
    #[error("{0}")]
    Smt(#[from] crate::decoded::primitives::PartialSmtError),
    #[error("{0}")]
    Asset(#[from] miden_protocol::errors::AssetError),
    #[error("{0}")]
    Vault(#[from] miden_protocol::errors::PartialAssetVaultError),
}

pub use proto::account::DecodedPartialAccount as PartialAccount;

impl Verify for PartialAccount {
    type Verified = miden_protocol::account::PartialAccount;
    type Error = PartialAccountError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.account_id.verify()?,
            self.nonce,
            self.code.verify()?,
            self.storage.verify()?,
            self.vault.verify()?,
            self.seed,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PartialAccountError {
    #[error("{0}")]
    Code(#[from] AccountCodeError),
    #[error("{0}")]
    AccountId(#[from] miden_protocol::errors::AccountIdError),
    #[error("{0}")]
    Account(#[from] miden_protocol::errors::AccountError),
    #[error("{0}")]
    Storage(#[from] PartialStorageError),
    #[error("{0}")]
    Vault(#[from] PartialVaultError),
}

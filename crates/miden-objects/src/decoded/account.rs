//! Domain construction for decoded account messages.
pub use proto::account::{
    DecodedAccountId as AccountId,
    DecodedAccountIdV1 as AccountIdV1,
    DecodedStorageSlotId as StorageSlotId,
};

use crate::{Verify, proto};

impl Verify for AccountId {
    type Verified = miden_protocol::account::AccountId;
    type Error = miden_protocol::errors::AccountIdError;

    fn verify(self) -> Result<Self::Verified, Self::Error> {
        match self.version {
            proto::account::account_id::DecodedVersion::V1(id) => {
                Ok(Self::Verified::V1(id.verify()?))
            },
        }
    }
}

impl Verify for AccountIdV1 {
    type Verified = miden_protocol::account::AccountIdV1;
    type Error = miden_protocol::errors::AccountIdError;

    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Self::Verified::try_from_elements(self.suffix, self.prefix)
    }
}

impl Verify for StorageSlotId {
    type Verified = miden_protocol::account::StorageSlotId;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(self.suffix, self.prefix))
    }
}

pub use proto::account::DecodedStorageMapEntry as StorageMapEntry;

impl Verify for StorageMapEntry {
    type Verified = (miden_protocol::account::StorageMapKey, miden_protocol::Word);
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((miden_protocol::account::StorageMapKey::from_raw(self.key), self.value))
    }
}

pub use proto::account::DecodedAccountCode as AccountCode;

impl Verify for AccountCode {
    type Verified = miden_protocol::account::AccountCode;
    type Error = AccountCodeError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let roots = self
            .procedure_roots
            .into_iter()
            .map(miden_protocol::account::AccountProcedureRoot::from_raw)
            .collect();
        Ok(Self::Verified::from_parts(alloc::sync::Arc::new(self.mast.verify()?), roots)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AccountCodeError {
    #[error("{0}")]
    Mast(#[from] miden_protocol::assembly::mast::MastForestError),
    #[error("{0}")]
    Code(#[from] miden_protocol::errors::AccountError),
}

pub use proto::account::DecodedAccountWitness as AccountWitness;

impl Verify for AccountWitness {
    type Verified = miden_protocol::block::account_tree::AccountWitness;
    type Error = AccountWitnessError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.witness_id.verify()?,
            self.commitment,
            self.path.verify()?,
        )?)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum AccountWitnessError {
    #[error("{0}")]
    AccountId(#[from] miden_protocol::errors::AccountIdError),
    #[error("invalid witness path: {0}")]
    Path(#[from] miden_protocol::crypto::merkle::MerkleError),
    #[error("invalid account witness: {0}")]
    Witness(#[from] miden_protocol::errors::AccountTreeError),
}

pub use proto::account::DecodedAccountVaultPatchEntry as AccountVaultPatchEntry;

impl Verify for AccountVaultPatchEntry {
    type Verified = (miden_protocol::asset::AssetId, miden_protocol::Word);
    type Error = miden_protocol::errors::AssetError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok((self.asset_id.try_into()?, self.value))
    }
}

pub use proto::account::DecodedAccountVaultPatch as AccountVaultPatch;

impl Verify for AccountVaultPatch {
    type Verified = miden_protocol::account::AccountVaultPatch;
    type Error = VaultPatchError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let mut entries = alloc::collections::BTreeMap::new();
        for entry in self.entries {
            let (id, value) = entry.verify()?;
            if entries.insert(id, value).is_some() {
                return Err(VaultPatchError::DuplicateAssetId(id));
            }
        }
        Ok(Self::Verified::new(entries)?)
    }
}
#[derive(Debug, thiserror::Error)]
pub enum VaultPatchError {
    #[error("invalid vault asset: {0}")]
    Asset(#[from] miden_protocol::errors::AssetError),
    #[error("duplicate vault asset ID {0}")]
    DuplicateAssetId(miden_protocol::asset::AssetId),
}

pub use proto::account::DecodedPrivateAccountUpdate as PrivateAccountUpdate;

impl Verify for PrivateAccountUpdate {
    type Verified = miden_protocol::account::AccountUpdateDetails;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::Private)
    }
}

pub use proto::account::DecodedAccountHeader as AccountHeader;

impl Verify for AccountHeader {
    type Verified = miden_protocol::account::AccountHeader;
    type Error = AccountHeaderError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        match self.version {
            proto::account::AccountVersion::V1 => {},
            proto::account::AccountVersion::Unspecified => {
                return Err(AccountHeaderError::UnspecifiedVersion);
            },
        }
        Ok(Self::Verified::new(
            self.account_id.verify()?,
            self.nonce.try_into().map_err(AccountHeaderError::Nonce)?,
            self.vault_root,
            self.storage_commitment,
            self.code_commitment,
        ))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum AccountHeaderError {
    #[error("{0}")]
    AccountId(#[from] miden_protocol::errors::AccountIdError),
    #[error("account header version is unspecified")]
    UnspecifiedVersion,
    #[error("invalid account nonce: {0}")]
    Nonce(#[source] <miden_protocol::Felt as TryFrom<u64>>::Error),
}

pub use proto::account::account_storage_header::DecodedStorageSlot as AccountStorageHeaderStorageSlot;

impl Verify for AccountStorageHeaderStorageSlot {
    type Verified = miden_protocol::account::StorageSlotHeader;
    type Error = StorageHeaderError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use miden_protocol::account::StorageSlotType;
        use proto::account::account_storage_header::storage_slot::DecodedContent;

        let name = miden_protocol::account::StorageSlotName::new(self.slot_name)?;
        let (slot_type, value) = match self.content {
            DecodedContent::Value(value) => (StorageSlotType::Value, value),
            DecodedContent::MapRoot(root) => (StorageSlotType::Map, root),
        };
        Ok(Self::Verified::new(name, slot_type, value))
    }
}
#[derive(Debug, thiserror::Error)]
pub enum StorageHeaderError {
    #[error("invalid storage header: {0}")]
    Header(#[from] miden_protocol::errors::AccountError),
    #[error("invalid storage slot name: {0}")]
    Name(#[from] miden_protocol::errors::StorageSlotNameError),
}

pub use proto::account::DecodedAccountStorageHeader as AccountStorageHeader;

impl Verify for AccountStorageHeader {
    type Verified = miden_protocol::account::AccountStorageHeader;
    type Error = StorageHeaderError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let slots = self.slots.into_iter().map(Verify::verify).collect::<Result<_, _>>()?;
        Ok(Self::Verified::new(slots)?)
    }
}

pub use proto::account::DecodedStorageMapPatchEntries as StorageMapPatchEntries;

impl Verify for StorageMapPatchEntries {
    type Verified = miden_protocol::account::StorageMapPatchEntries;
    type Error = StorageMapPatchError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let mut entries = alloc::collections::BTreeMap::new();
        for entry in self.entries {
            let (key, value) = entry.verify().expect("infallible storage map entry");
            if entries.insert(key, value).is_some() {
                return Err(StorageMapPatchError::DuplicateKey(key));
            }
        }
        Ok(Self::Verified::from_raw(entries))
    }
}

pub use proto::account::DecodedStorageMapPatch as StorageMapPatch;

impl Verify for StorageMapPatch {
    type Verified = miden_protocol::account::StorageMapPatch;
    type Error = StorageMapPatchError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use proto::account::storage_map_patch::DecodedOperation;
        match self.operation {
            DecodedOperation::Create(entries) => {
                Ok(Self::Verified::Create { entries: entries.verify()? })
            },
            DecodedOperation::Update(entries) => {
                let entries = entries.verify()?;
                if entries.is_empty() {
                    return Err(StorageMapPatchError::EmptyUpdate);
                }
                Ok(Self::Verified::Update { entries })
            },
            DecodedOperation::Remove(()) => Ok(Self::Verified::Remove),
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum StorageMapPatchError {
    #[error("entries must be non-empty for an update operation")]
    EmptyUpdate,
    #[error("duplicate storage map key {0:?}")]
    DuplicateKey(miden_protocol::account::StorageMapKey),
}

pub use proto::account::DecodedStorageValuePatch as StorageValuePatch;

impl Verify for StorageValuePatch {
    type Verified = miden_protocol::account::StorageValuePatch;
    type Error = core::convert::Infallible;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use proto::account::storage_value_patch::DecodedOperation;
        Ok(match self.operation {
            DecodedOperation::Create(value) => Self::Verified::Create { value },
            DecodedOperation::Update(value) => Self::Verified::Update { value },
            DecodedOperation::Remove(()) => Self::Verified::Remove,
        })
    }
}

pub use proto::account::DecodedStorageSlotPatch as StorageSlotPatch;

impl Verify for StorageSlotPatch {
    type Verified = (
        miden_protocol::account::StorageSlotName,
        miden_protocol::account::StorageSlotPatch,
    );
    type Error = StoragePatchError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use miden_protocol::account::{StorageSlotName, StorageSlotPatch};
        use proto::account::storage_slot_patch::DecodedPatch;
        let name = StorageSlotName::new(self.slot_name)?;
        let patch = match self.patch {
            DecodedPatch::Value(value) => {
                StorageSlotPatch::Value(value.verify().expect("infallible value patch"))
            },
            DecodedPatch::Map(map) => StorageSlotPatch::Map(map.verify()?),
        };
        Ok((name, patch))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoragePatchError {
    #[error("invalid storage slot name: {0}")]
    Name(#[from] miden_protocol::errors::StorageSlotNameError),
    #[error("invalid storage map patch: {0}")]
    Map(#[from] StorageMapPatchError),
    #[error("invalid storage patch: {0}")]
    Storage(#[from] miden_protocol::errors::AccountPatchError),
}

pub use proto::account::DecodedAccountStoragePatch as AccountStoragePatch;

impl Verify for AccountStoragePatch {
    type Verified = miden_protocol::account::AccountStoragePatch;
    type Error = StoragePatchError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        let slots = self
            .slots
            .into_iter()
            .map(Verify::verify)
            .collect::<Result<alloc::vec::Vec<_>, _>>()?;
        Ok(Self::Verified::from_entries(slots)?)
    }
}

pub use proto::account::DecodedAccountPatch as AccountPatch;

impl Verify for AccountPatch {
    type Verified = miden_protocol::account::AccountPatch;
    type Error = AccountPatchError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        if self.version != proto::account::AccountPatchVersion::V1 {
            return Err(AccountPatchError::UnspecifiedVersion);
        }
        Ok(Self::Verified::new(
            self.account_id.verify()?,
            self.storage.verify()?,
            self.vault.verify()?,
            self.code.map(Verify::verify).transpose()?,
            self.final_nonce,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AccountPatchError {
    #[error("{0}")]
    AccountId(#[from] miden_protocol::errors::AccountIdError),
    #[error("account patch version is unspecified")]
    UnspecifiedVersion,
    #[error("{0}")]
    Storage(#[from] StoragePatchError),
    #[error("{0}")]
    Vault(#[from] VaultPatchError),
    #[error("{0}")]
    Code(#[from] AccountCodeError),
    #[error("{0}")]
    Patch(#[from] miden_protocol::errors::AccountPatchError),
}

pub use proto::account::DecodedAccountUpdateDetails as AccountUpdateDetails;

impl Verify for AccountUpdateDetails {
    type Verified = miden_protocol::account::AccountUpdateDetails;
    type Error = AccountPatchError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        use proto::account::account_update_details::DecodedUpdate;
        match self.update {
            DecodedUpdate::Private(value) => Ok(value.verify().expect("infallible private update")),
            DecodedUpdate::Public(patch) => Ok(Self::Verified::Public(patch.verify()?)),
        }
    }
}

pub use proto::account::DecodedPartialStorageMap as PartialStorageMap;

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
    Smt(#[from] super::primitives::PartialSmtError),
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
    Smt(#[from] super::primitives::PartialSmtError),
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

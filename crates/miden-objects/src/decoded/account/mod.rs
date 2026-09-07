//! Domain construction for decoded account messages.

#[cfg(test)]
pub(crate) mod test_utils;

mod core;
pub use core::{
    AccountCode,
    AccountCodeError,
    AccountHeader,
    AccountHeaderError,
    AccountId,
    AccountIdV1,
    AccountWitness,
    AccountWitnessError,
};

mod storage;
pub use storage::{
    AccountStorageHeader,
    AccountStorageHeaderStorageSlot,
    StorageHeaderError,
    StorageMapEntry,
    StorageSlotId,
};

mod patch;
pub use patch::{
    AccountPatch,
    AccountPatchError,
    AccountStoragePatch,
    AccountUpdateDetails,
    AccountVaultPatch,
    AccountVaultPatchEntry,
    PrivateAccountUpdate,
    StorageMapPatch,
    StorageMapPatchEntries,
    StorageMapPatchError,
    StoragePatchError,
    StorageSlotPatch,
    StorageValuePatch,
    VaultPatchError,
};

mod partial;
pub use partial::{
    PartialAccount,
    PartialAccountError,
    PartialStorage,
    PartialStorageError,
    PartialStorageMap,
    PartialStorageMapError,
    PartialVault,
    PartialVaultError,
};

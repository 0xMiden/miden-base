use alloc::vec::Vec;

use miden_crypto::merkle::SmtProof;
use vm_core::utils::{Deserializable, Serializable};
use vm_processor::Digest;

use super::{AccountStorage, AccountStorageHeader, StorageSlot};
use crate::AccountError;

/// A partial representation of an account storage, containing only a subset of the storage data.
///
/// Partial storage is used to provide verifiable access to specific segments of account storage
/// without the need to provide the full storage data. It contains all needed parts for loading
/// account storage data into the transaction kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialStorage {
    /// Commitment of the account's storage slots.
    commitment: Digest,
    /// Account's storage heaer, containing top-level slot values.
    header: AccountStorageHeader,
    /// Merkle proofs for a subset of the account's storage maps keys
    storage_map_proofs: Vec<SmtProof>,
}

impl PartialStorage {
    /// Returns a new instance of partial storage with the specified header and storage map proofs.
    ///
    /// The storage commitment is computed during instantiation based on the provided header.
    pub fn new(header: AccountStorageHeader, storage_map_proofs: Vec<SmtProof>) -> Self {
        let commitment = header.compute_commitment();
        PartialStorage { header, storage_map_proofs, commitment }
    }

    /// Returns a reference to the storage map proofs of this partial storage.
    pub fn storage_map_proofs(&self) -> &[SmtProof] {
        &self.storage_map_proofs
    }

    /// Returns a reference to the header of this partial storage.
    pub fn header(&self) -> &AccountStorageHeader {
        &self.header
    }

    /// Returns the commitment of this partial storage.
    pub fn commitment(&self) -> Digest {
        self.commitment
    }

    /// Returns the value of the storage slot at the specified slot index.
    ///
    /// # Errors:
    /// - If the index is out of bounds
    pub fn get_item(&self, index: u8) -> Result<Digest, AccountError> {
        self.header.slot(index as usize).map(|(_type, value)| value.into())
    }

    // TODO: Add from account storage with (slot/[key])?
}

impl From<&AccountStorage> for PartialStorage {
    /// Converts a full account storage into a partial storage representation.
    ///
    /// This creates a partial storage that contains proofs for all key-value pairs
    /// in all map slots of the account storage.
    fn from(account_storage: &AccountStorage) -> Self {
        let mut storage_map_proofs = Vec::with_capacity(account_storage.slots().len());
        for slot in account_storage.slots() {
            if let StorageSlot::Map(map) = slot {
                let proofs: Vec<SmtProof> = map.entries().map(|(key, _)| map.open(key)).collect();
                storage_map_proofs.extend(proofs);
            }
        }

        let header: AccountStorageHeader = account_storage.to_header();
        let commitment = header.compute_commitment();
        PartialStorage { header, storage_map_proofs, commitment }
    }
}

impl Serializable for PartialStorage {
    fn write_into<W: vm_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(&self.header);
        target.write(&self.storage_map_proofs);
    }
}

impl Deserializable for PartialStorage {
    fn read_from<R: vm_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, vm_processor::DeserializationError> {
        let header: AccountStorageHeader = source.read()?;
        let storage_map_proofs: Vec<SmtProof> = source.read()?;

        let commitment = header.compute_commitment();

        Ok(PartialStorage { header, storage_map_proofs, commitment })
    }
}

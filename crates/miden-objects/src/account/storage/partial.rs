use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
};

use miden_crypto::{
    Word,
    merkle::{InnerNodeInfo, SmtLeaf},
};
use vm_core::utils::{Deserializable, Serializable};

use super::{AccountStorage, AccountStorageHeader, StorageSlot};
use crate::account::PartialStorageMap;

/// A partial representation of an account storage, containing only a subset of the storage data.
///
/// Partial storage is used to provide verifiable access to specific segments of account storage
/// without the need to provide the full storage data. It contains all needed parts for loading
/// account storage data into the transaction kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartialStorage {
    /// Commitment of the account's storage slots.
    commitment: Word,
    /// Account storage header.
    header: AccountStorageHeader,
    /// Storage map SMTs indexed by their root, containing a subset of the elements from the
    /// complete storage map.
    maps: BTreeMap<Word, PartialStorageMap>,
}

impl PartialStorage {
    /// Returns a new instance of partial storage with the specified header and storage map SMTs.
    ///
    /// The storage commitment is computed during instantiation based on the provided header.
    /// Additionally, this function validates that the passed SMTs correspond to one of the map
    /// roots in the storage header.
    pub fn new(
        storage_header: AccountStorageHeader,
        storage_maps: impl Iterator<Item = PartialStorageMap>,
    ) -> Result<Self, String> {
        let storage_map_roots: BTreeSet<_> = storage_header.map_slot_roots().collect();
        let mut maps = BTreeMap::new();
        for smt in storage_maps {
            // Check that the passed storage map partial SMT has a matching map slot root
            if storage_map_roots.contains(&smt.root()) {
                return Err(String::from("todo"));
            }
            maps.insert(smt.root(), smt);
        }

        let commitment = storage_header.compute_commitment();
        Ok(Self { commitment, header: storage_header, maps })
    }

    /// Returns a reference to the header of this partial storage.
    pub fn header(&self) -> &AccountStorageHeader {
        &self.header
    }

    /// Returns the commitment of this partial storage.
    pub fn commitment(&self) -> Word {
        self.commitment
    }

    // TODO: Add from account storage with (slot/[key])?

    // ITERATORS
    // --------------------------------------------------------------------------------------------

    /// Returns an iterator over inner nodes of all storage map proofs contained in this
    /// partial storage.
    pub fn inner_nodes(&self) -> impl Iterator<Item = InnerNodeInfo> {
        self.maps.iter().flat_map(|(_, map)| map.inner_nodes())
    }

    /// Iterator over every [`PartialStorageMap`] in this partial storage.
    pub fn maps(&self) -> impl Iterator<Item = &PartialStorageMap> + '_ {
        self.maps.values()
    }

    /// Iterator over all tracked, non‑empty leaves across every map.
    pub fn leaves(&self) -> impl Iterator<Item = &SmtLeaf> + '_ {
        self.maps().flat_map(|map| map.leaves()).map(|(_, leaf)| leaf)
    }
}

impl From<&AccountStorage> for PartialStorage {
    /// Converts a full account storage into a partial storage representation.
    ///
    /// This creates a partial storage that contains proofs for all key-value pairs
    /// in all map slots of the account storage.
    fn from(account_storage: &AccountStorage) -> Self {
        let mut map_smts = BTreeMap::new();
        for slot in account_storage.slots() {
            if let StorageSlot::Map(map) = slot {
                let smt: PartialStorageMap = map.clone().into();
                map_smts.insert(smt.root(), smt);
            }
        }

        let header: AccountStorageHeader = account_storage.to_header();
        let commitment = header.compute_commitment();
        PartialStorage { header, maps: map_smts, commitment }
    }
}

impl Serializable for PartialStorage {
    fn write_into<W: vm_core::utils::ByteWriter>(&self, target: &mut W) {
        target.write(&self.header);
        target.write(&self.maps);
    }
}

impl Deserializable for PartialStorage {
    fn read_from<R: vm_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, vm_processor::DeserializationError> {
        let header: AccountStorageHeader = source.read()?;
        let map_smts: BTreeMap<Word, PartialStorageMap> = source.read()?;

        let commitment = header.compute_commitment();

        Ok(PartialStorage { header, maps: map_smts, commitment })
    }
}

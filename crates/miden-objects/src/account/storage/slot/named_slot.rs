use crate::Word;
use crate::account::storage::slot::SlotNameId;
use crate::account::{SlotName, StorageMap, StorageSlot};

// TODO(named_slots): Docs + separators for the entire module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedStorageSlot {
    /// The name of the storage slot.
    name: SlotName,
    /// The cached [`SlotNameId`] of the slot name. These must always be consistent with each
    /// other.
    ///
    /// This is cached so that the `Ord` implementation can use the computed name ID instead of
    /// having to hash the slot name on every comparison operation.
    name_id: SlotNameId,
    /// The underlying storage slot.
    slot: StorageSlot,
}

impl NamedStorageSlot {
    pub fn new(name: SlotName, slot: StorageSlot) -> Self {
        let name_id = name.compute_id();

        Self { name, name_id, slot }
    }

    pub fn with_value(name: SlotName, value: Word) -> Self {
        Self::new(name, StorageSlot::Value(value))
    }

    pub fn with_map(name: SlotName, map: StorageMap) -> Self {
        Self::new(name, StorageSlot::Map(map))
    }

    pub fn name(&self) -> &SlotName {
        &self.name
    }

    pub fn name_id(&self) -> SlotNameId {
        self.name_id
    }

    pub fn storage_slot(&self) -> &StorageSlot {
        &self.slot
    }

    pub fn storage_slot_mut(&mut self) -> &mut StorageSlot {
        &mut self.slot
    }

    pub fn into_parts(self) -> (SlotName, SlotNameId, StorageSlot) {
        (self.name, self.name_id, self.slot)
    }
}

impl Ord for NamedStorageSlot {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.name_id.cmp(&other.name_id)
    }
}

impl PartialOrd for NamedStorageSlot {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl crate::utils::serde::Serializable for NamedStorageSlot {
    fn write_into<W: crate::utils::serde::ByteWriter>(&self, target: &mut W) {
        target.write(&self.name);
        target.write(&self.slot);
    }

    fn get_size_hint(&self) -> usize {
        self.name.get_size_hint() + self.storage_slot().get_size_hint()
    }
}

impl crate::utils::serde::Deserializable for NamedStorageSlot {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, crate::utils::serde::DeserializationError> {
        let name: SlotName = source.read()?;
        let slot: StorageSlot = source.read()?;

        Ok(Self::new(name, slot))
    }
}

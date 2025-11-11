use crate::Word;
use crate::account::storage::slot::SlotNameId;
use crate::account::{SlotName, StorageMap, StorageSlot, StorageSlotType};

/// A [`StorageSlot`] identified by a [`SlotName`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedStorageSlot {
    /// The name of the storage slot.
    name: SlotName,
    /// The cached [`SlotNameId`] of the slot name. This field must always be consistent with the
    /// slot name.
    ///
    /// This is cached so that the `Ord` implementation can use the computed name ID instead of
    /// having to hash the slot name on every comparison operation.
    name_id: SlotNameId,
    /// The underlying storage slot.
    slot: StorageSlot,
}

impl NamedStorageSlot {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`NamedStorageSlot`] with the given [`SlotName`] and [`StorageSlot`].
    pub fn new(name: SlotName, slot: StorageSlot) -> Self {
        let name_id = name.compute_id();

        Self { name, name_id, slot }
    }

    /// Creates a new [`NamedStorageSlot`] with the given [`SlotName`] and the `value` wrapped into
    /// a [`StorageSlot::Value`].
    pub fn with_value(name: SlotName, value: Word) -> Self {
        Self::new(name, StorageSlot::Value(value))
    }

    /// Creates a new [`NamedStorageSlot`] with the given [`SlotName`] and
    /// [`StorageSlot::empty_value`].
    pub fn with_empty_value(name: SlotName) -> Self {
        Self::new(name, StorageSlot::empty_value())
    }

    /// Creates a new [`NamedStorageSlot`] with the given [`SlotName`] and the `map` wrapped into a
    /// [`StorageSlot::Map`]
    pub fn with_map(name: SlotName, map: StorageMap) -> Self {
        Self::new(name, StorageSlot::Map(map))
    }

    /// Creates a new [`NamedStorageSlot`] with the given [`SlotName`] and
    /// [`StorageSlot::empty_map`].
    pub fn with_empty_map(name: SlotName) -> Self {
        Self::new(name, StorageSlot::empty_map())
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`SlotName`] by which the [`NamedStorageSlot`] is identified.
    pub fn name(&self) -> &SlotName {
        &self.name
    }

    /// Returns the [`SlotNameId`] by which the [`NamedStorageSlot`] is identified.
    pub fn name_id(&self) -> SlotNameId {
        self.name_id
    }

    /// Returns this storage slot value as a [Word]
    ///
    /// Returns:
    /// - For [`StorageSlot::Value`] the value.
    /// - For [`StorageSlot::Map`] the root of the [StorageMap].
    pub fn value(&self) -> Word {
        self.storage_slot().value()
    }

    /// Returns a reference to the [`StorageSlot`] contained in this [`NamedStorageSlot`].
    pub fn storage_slot(&self) -> &StorageSlot {
        &self.slot
    }

    /// Returns the [`StorageSlotType`] of this [`NamedStorageSlot`].
    pub fn slot_type(&self) -> StorageSlotType {
        self.slot.slot_type()
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Returns a mutable reference to the [`StorageSlot`] contained in this [`NamedStorageSlot`].
    pub fn storage_slot_mut(&mut self) -> &mut StorageSlot {
        &mut self.slot
    }

    /// Consumes self and returns the underlying parts.
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

// SERIALIZATION
// ================================================================================================

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

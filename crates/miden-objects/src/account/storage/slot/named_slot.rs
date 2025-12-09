use crate::Word;
use crate::account::storage::slot::StorageSlotId;
use crate::account::{StorageMap, StorageSlot, StorageSlotName, StorageSlotType};

/// An individual storage slot in [`AccountStorage`](crate::account::AccountStorage).
///
/// This consists of a [`StorageSlotName`] that uniquely identifies the slot and its [`StorageSlot`]
/// content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedStorageSlot {
    /// The name of the storage slot.
    name: StorageSlotName,
    /// The cached [`StorageSlotId`] of the slot name. This field must always be consistent with
    /// the slot name.
    ///
    /// This is cached so that the `Ord` implementation can use the computed slot ID instead of
    /// having to hash the slot name on every comparison operation.
    slot_id: StorageSlotId,
    /// The underlying storage slot.
    slot: StorageSlot,
}

impl NamedStorageSlot {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`NamedStorageSlot`] with the given [`StorageSlotName`] and [`StorageSlot`].
    pub fn new(name: StorageSlotName, slot: StorageSlot) -> Self {
        let slot_id = name.compute_id();

        Self { name, slot_id, slot }
    }

    /// Creates a new [`NamedStorageSlot`] with the given [`StorageSlotName`] and the `value`
    /// wrapped into a [`StorageSlot::Value`].
    pub fn with_value(name: StorageSlotName, value: Word) -> Self {
        Self::new(name, StorageSlot::Value(value))
    }

    /// Creates a new [`NamedStorageSlot`] with the given [`StorageSlotName`] and
    /// [`StorageSlot::empty_value`].
    pub fn with_empty_value(name: StorageSlotName) -> Self {
        Self::new(name, StorageSlot::empty_value())
    }

    /// Creates a new [`NamedStorageSlot`] with the given [`StorageSlotName`] and the `map` wrapped
    /// into a [`StorageSlot::Map`]
    pub fn with_map(name: StorageSlotName, map: StorageMap) -> Self {
        Self::new(name, StorageSlot::Map(map))
    }

    /// Creates a new [`NamedStorageSlot`] with the given [`StorageSlotName`] and
    /// [`StorageSlot::empty_map`].
    pub fn with_empty_map(name: StorageSlotName) -> Self {
        Self::new(name, StorageSlot::empty_map())
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`StorageSlotName`] by which the [`NamedStorageSlot`] is identified.
    pub fn name(&self) -> &StorageSlotName {
        &self.name
    }

    /// Returns the [`StorageSlotId`] by which the [`NamedStorageSlot`] is identified.
    pub fn slot_id(&self) -> StorageSlotId {
        self.slot_id
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
    pub fn into_parts(self) -> (StorageSlotName, StorageSlotId, StorageSlot) {
        (self.name, self.slot_id, self.slot)
    }
}

impl Ord for NamedStorageSlot {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.slot_id.cmp(&other.slot_id)
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
        let name: StorageSlotName = source.read()?;
        let slot: StorageSlot = source.read()?;

        Ok(Self::new(name, slot))
    }
}

use crate::Word;
use crate::account::storage::slot::StorageSlotId;
use crate::account::{StorageMap, StorageSlotContent, StorageSlotName, StorageSlotType};

/// An individual storage slot in [`AccountStorage`](crate::account::AccountStorage).
///
/// This consists of a [`StorageSlotName`] that uniquely identifies the slot and its
/// [`StorageSlotContent`] content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageSlot {
    /// The name of the storage slot.
    name: StorageSlotName,
    /// The cached [`StorageSlotId`] of the slot name. This field must always be consistent with
    /// the slot name.
    ///
    /// This is cached so that the `Ord` implementation can use the computed slot ID instead of
    /// having to hash the slot name on every comparison operation.
    slot_id: StorageSlotId,
    /// The underlying storage slot.
    content: StorageSlotContent,
}

impl StorageSlot {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Creates a new [`StorageSlot`] with the given [`StorageSlotName`] and
    /// [`StorageSlotContent`].
    pub fn new(name: StorageSlotName, content: StorageSlotContent) -> Self {
        let slot_id = name.compute_id();

        Self { name, slot_id, content }
    }

    /// Creates a new [`StorageSlot`] with the given [`StorageSlotName`] and the `value`
    /// wrapped into a [`StorageSlotContent::Value`].
    pub fn with_value(name: StorageSlotName, value: Word) -> Self {
        Self::new(name, StorageSlotContent::Value(value))
    }

    /// Creates a new [`StorageSlot`] with the given [`StorageSlotName`] and
    /// [`StorageSlotContent::empty_value`].
    pub fn with_empty_value(name: StorageSlotName) -> Self {
        Self::new(name, StorageSlotContent::empty_value())
    }

    /// Creates a new [`StorageSlot`] with the given [`StorageSlotName`] and the `map` wrapped
    /// into a [`StorageSlotContent::Map`]
    pub fn with_map(name: StorageSlotName, map: StorageMap) -> Self {
        Self::new(name, StorageSlotContent::Map(map))
    }

    /// Creates a new [`StorageSlot`] with the given [`StorageSlotName`] and
    /// [`StorageSlotContent::empty_map`].
    pub fn with_empty_map(name: StorageSlotName) -> Self {
        Self::new(name, StorageSlotContent::empty_map())
    }

    // ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the [`StorageSlotName`] by which the [`StorageSlot`] is identified.
    pub fn name(&self) -> &StorageSlotName {
        &self.name
    }

    /// Returns the [`StorageSlotId`] by which the [`StorageSlot`] is identified.
    pub fn slot_id(&self) -> StorageSlotId {
        self.slot_id
    }

    /// Returns this storage slot value as a [Word]
    ///
    /// Returns:
    /// - For [`StorageSlotContent::Value`] the value.
    /// - For [`StorageSlotContent::Map`] the root of the [StorageMap].
    pub fn value(&self) -> Word {
        self.storage_slot().value()
    }

    /// TODO: Rename to content.
    /// Returns a reference to the [`StorageSlotContent`] contained in this [`StorageSlot`].
    pub fn storage_slot(&self) -> &StorageSlotContent {
        &self.content
    }

    /// Returns the [`StorageSlotType`] of this [`StorageSlot`].
    pub fn slot_type(&self) -> StorageSlotType {
        self.content.slot_type()
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Returns a mutable reference to the [`StorageSlotContent`] contained in this
    /// [`StorageSlot`].
    pub fn storage_slot_mut(&mut self) -> &mut StorageSlotContent {
        &mut self.content
    }

    /// Consumes self and returns the underlying parts.
    pub fn into_parts(self) -> (StorageSlotName, StorageSlotId, StorageSlotContent) {
        (self.name, self.slot_id, self.content)
    }
}

impl Ord for StorageSlot {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.slot_id.cmp(&other.slot_id)
    }
}

impl PartialOrd for StorageSlot {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// SERIALIZATION
// ================================================================================================

impl crate::utils::serde::Serializable for StorageSlot {
    fn write_into<W: crate::utils::serde::ByteWriter>(&self, target: &mut W) {
        target.write(&self.name);
        target.write(&self.content);
    }

    fn get_size_hint(&self) -> usize {
        self.name.get_size_hint() + self.storage_slot().get_size_hint()
    }
}

impl crate::utils::serde::Deserializable for StorageSlot {
    fn read_from<R: miden_core::utils::ByteReader>(
        source: &mut R,
    ) -> Result<Self, crate::utils::serde::DeserializationError> {
        let name: StorageSlotName = source.read()?;
        let slot: StorageSlotContent = source.read()?;

        Ok(Self::new(name, slot))
    }
}

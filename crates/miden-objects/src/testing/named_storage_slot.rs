use crate::Word;
use crate::account::{NamedStorageSlot, SlotName, StorageMap};

impl NamedStorageSlot {
    /// Returns a [`NamedStorageSlot`] of type value with a name derived from the `index`.
    pub fn with_test_value(index: usize, value: Word) -> Self {
        Self::with_value(SlotName::new_test(index), value)
    }

    /// Returns a [`NamedStorageSlot`] of type map with a name derived from the `index`.
    pub fn with_test_map(index: usize, map: StorageMap) -> Self {
        Self::with_map(SlotName::new_test(index), map)
    }
}

use crate::Word;
use crate::account::{NamedStorageSlot, SlotName, StorageMap};

impl NamedStorageSlot {
    pub fn randomly_named_value(value: Word) -> Self {
        Self::with_value(SlotName::random(), value)
    }

    pub fn randomly_named_map(map: StorageMap) -> Self {
        Self::with_map(SlotName::random(), map)
    }
}

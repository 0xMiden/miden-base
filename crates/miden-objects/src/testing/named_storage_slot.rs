use crate::Word;
use crate::account::{NamedStorageSlot, SlotName, StorageMap};

impl NamedStorageSlot {
    pub fn randomly_named_value(value: Word) -> Self {
        Self::with_value(random_slot_name(), value)
    }

    pub fn randomly_named_map(map: StorageMap) -> Self {
        Self::with_map(random_slot_name(), map)
    }
}

fn random_slot_name() -> SlotName {
    SlotName::new(format!("miden::test::slot{}", rand::random::<u64>()))
        .expect("slot name should be valid")
}

use crate::account::SlotName;

impl SlotName {
    /// Returns a new slot name with the format `"miden::test::slot{index}"`.
    pub fn new_test(index: usize) -> Self {
        Self::new(format!("miden::test::slot{index}")).expect("slot name should be valid")
    }
}

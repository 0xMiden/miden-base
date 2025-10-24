use crate::account::SlotName;

impl SlotName {
    /// Returns a new slot name with the format `"miden::test::slot{random_value}"`.
    pub fn random() -> Self {
        Self::new_test(rand::random::<u64>())
    }

    /// Returns a new slot name with the format `"miden::test::slot{index}"`.
    pub fn new_test(index: u64) -> Self {
        Self::new(format!("miden::test::slot{index}")).expect("slot name should be valid")
    }
}

use alloc::vec::Vec;

use miden_objects::AccountError;
use miden_objects::account::{AccountCode, AccountComponent, StorageSlot};

use crate::testing::mock_account_code::MockAccountCodeExt;

// ACCOUNT MOCK COMPONENT
// ================================================================================================

/// Creates a mock [`Library`](miden_objects::assembly::Library) which can be used to assemble
/// programs and as a library to create a mock [`AccountCode`](miden_objects::account::AccountCode)
/// interface. Transaction and note scripts that make use of this interface should be assembled with
/// this.
///
/// This component supports all [`AccountType`](miden_objects::account::AccountType)s for testing
/// purposes.
pub struct MockAccountComponent {
    storage_slots: Vec<StorageSlot>,
}

impl MockAccountComponent {
    // CONSTRUCTORS
    // --------------------------------------------------------------------------------------------

    /// Constructs a [`MockAccountComponent`] with empty storage.
    pub fn with_empty_slots() -> Self {
        Self::new(vec![])
    }

    /// Constructs a [`MockAccountComponent`] with the provided storage slots.
    ///
    /// # Panics
    ///
    /// Panics if the number of slots exceeds the maximum.
    pub fn with_slots(storage_slots: Vec<StorageSlot>) -> Self {
        Self::new(storage_slots)
    }

    // HELPERS
    // --------------------------------------------------------------------------------------------

    fn new(storage_slots: Vec<StorageSlot>) -> Self {
        // Check that we have less than 256 storage slots.
        u8::try_from(storage_slots.len())
            .map_err(|_| AccountError::StorageTooManySlots(storage_slots.len() as u64))
            .expect("too many storage slots");

        Self { storage_slots }
    }
}

impl From<MockAccountComponent> for AccountComponent {
    fn from(mock_component: MockAccountComponent) -> Self {
        AccountComponent::new(AccountCode::mock_library(), mock_component.storage_slots)
          .expect("account mock component should satisfy the requirements of a valid account component")
          .with_supports_all_types()
    }
}

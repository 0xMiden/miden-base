use alloc::vec::Vec;

use crate::AccountError;
use crate::account::{AccountComponent, StorageSlot};
use crate::assembly::diagnostics::NamedSource;
use crate::assembly::{Assembler, Library};
use crate::testing::account_code::MOCK_ACCOUNT_CODE;

// ACCOUNT MOCK COMPONENT
// ================================================================================================

/// Creates a mock [`Library`] which can be used to assemble programs and as a library to create a
/// mock [`AccountCode`](crate::account::AccountCode) interface. Transaction and note scripts that
/// make use of this interface should be assembled with this.
///
/// This component supports all [`AccountType`](crate::account::AccountType)s for testing purposes.
pub struct AccountMockComponent {
    library: Library,
    storage_slots: Vec<StorageSlot>,
}

impl AccountMockComponent {
    fn new(assembler: Assembler, storage_slots: Vec<StorageSlot>) -> Result<Self, AccountError> {
        // Check that we have less than 256 storage slots.
        u8::try_from(storage_slots.len())
            .map_err(|_| AccountError::StorageTooManySlots(storage_slots.len() as u64))?;

        let source = NamedSource::new("test::account", MOCK_ACCOUNT_CODE);
        let library = assembler
            .assemble_library([source])
            .map_err(AccountError::AccountComponentAssemblyError)?;

        Ok(Self { library, storage_slots })
    }

    pub fn new_with_empty_slots(assembler: Assembler) -> Result<Self, AccountError> {
        Self::new(assembler, vec![])
    }

    pub fn new_with_slots(
        assembler: Assembler,
        storage_slots: Vec<StorageSlot>,
    ) -> Result<Self, AccountError> {
        Self::new(assembler, storage_slots)
    }
}

impl From<AccountMockComponent> for Library {
    fn from(mock_component: AccountMockComponent) -> Self {
        mock_component.library
    }
}

impl From<AccountMockComponent> for AccountComponent {
    fn from(mock_component: AccountMockComponent) -> Self {
        AccountComponent::new(mock_component.library, mock_component.storage_slots)
            .expect("account mock component should satisfy the requirements of a valid account component")
            .with_supports_all_types()
    }
}

// MOCK AUTH COMPONENTS
// ================================================================================================

const NOOP_AUTH_CODE: &str = "
    use.miden::account

    export.auth__noop
        push.0 drop
    end
";

/// Creates a mock authentication [`AccountComponent`] for testing purposes.
///
/// The component defines an `auth__noop` procedure that does nothing (always succeeds).
pub struct NoopAuthComponent {
    pub library: Library,
}

impl NoopAuthComponent {
    pub fn new(assembler: Assembler) -> Result<Self, AccountError> {
        let library = assembler
            .assemble_library([NOOP_AUTH_CODE])
            .map_err(AccountError::AccountComponentAssemblyError)?;
        Ok(Self { library })
    }
}

impl From<NoopAuthComponent> for AccountComponent {
    fn from(mock_component: NoopAuthComponent) -> Self {
        AccountComponent::new(mock_component.library, vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}

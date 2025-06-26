use alloc::vec::Vec;

use crate::{
    AccountError,
    account::{AccountComponent, StorageSlot},
    assembly::{Assembler, Library, diagnostics::NamedSource},
    testing::account_code::MOCK_ACCOUNT_CODE,
};

// ACCOUNT COMPONENT ASSEMBLY CODE
// ================================================================================================

pub const BASIC_WALLET_CODE: &str = "
    export.::miden::contracts::wallets::basic::receive_asset
    export.::miden::contracts::wallets::basic::create_note
    export.::miden::contracts::wallets::basic::move_asset_to_note
";

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

// MOCK AUTH COMPONENT
// ================================================================================================

/// Creates a mock authentication [`AccountComponent`] for testing purposes. It only increments the nonce.
pub struct MockAuthComponent {
    library: Library,
}

impl MockAuthComponent {
    /// Creates a new MockAuthComponent using the provided assembler.
    pub fn from_assembler(assembler: Assembler) -> Result<Self, AccountError> {
        let library = assembler
            .assemble_library([AUTH_CODE])
            .map_err(AccountError::AccountComponentAssemblyError)?;

        Ok(Self { library })
    }
}

impl From<MockAuthComponent> for AccountComponent {
    fn from(mock_component: MockAuthComponent) -> Self {
        AccountComponent::new(mock_component.library, vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}

const AUTH_CODE: &str = "
    use.miden::account

    export.auth
        push.1 exec.account::incr_nonce
    end
";

const NOOP_AUTH_CODE: &str = "
    use.miden::account

    export.auth
        push.0 drop
    end
";

const CONDITIONAL_AUTH_CODE: &str = "
    use.miden::account

    export.noop
        push.0
        exec.account::get_item

        push.99.99.99.99
        eqw

        # If 99.99.99.99 is in storage, increment nonce
        if.true
            push.1 exec.account::incr_nonce
        end
        dropw dropw dropw dropw
    end
";

pub struct NoopAuthComponent {
    library: Library,
}

impl NoopAuthComponent {
    pub fn from_assembler(assembler: Assembler) -> Result<Self, AccountError> {
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

pub struct ConditionalAuthComponent {
    library: Library,
}

impl ConditionalAuthComponent {
    pub fn from_assembler(assembler: Assembler) -> Result<Self, AccountError> {
        let library = assembler
            .assemble_library([CONDITIONAL_AUTH_CODE])
            .map_err(AccountError::AccountComponentAssemblyError)?;
        Ok(Self { library })
    }
}

impl From<ConditionalAuthComponent> for AccountComponent {
    fn from(mock_component: ConditionalAuthComponent) -> Self {
        AccountComponent::new(mock_component.library, vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}

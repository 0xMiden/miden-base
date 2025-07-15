use alloc::{string::String, vec::Vec};

use crate::{
    AccountError,
    account::{AccountComponent, StorageSlot},
    assembly::{Assembler, Library, diagnostics::NamedSource},
    testing::account_code::MOCK_ACCOUNT_CODE,
    utils::sync::LazyLock,
};

// ACCOUNT COMPONENT ASSEMBLY CODE
// ================================================================================================

pub const BASIC_WALLET_CODE: &str = "
    export.::miden::contracts::wallets::basic::receive_asset
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

// MOCK AUTH COMPONENTS
// ================================================================================================

/// Creates a mock authentication [`AccountComponent`] for testing purposes.
///
/// The component defines an `auth__basic` procedure that always increments the nonce by 1.
pub struct IncrNonceAuthComponent {
    library: Library,
}

impl IncrNonceAuthComponent {
    /// Creates a new IncrNonceAuthComponent using the provided assembler.
    pub fn new(assembler: Assembler) -> Result<Self, AccountError> {
        let library = assembler
            .assemble_library([INCR_NONCE_AUTH_CODE])
            .map_err(AccountError::AccountComponentAssemblyError)?;

        Ok(Self { library })
    }
}

impl From<IncrNonceAuthComponent> for AccountComponent {
    fn from(mock_component: IncrNonceAuthComponent) -> Self {
        AccountComponent::new(mock_component.library, vec![])
            .expect("component should be valid")
            .with_supports_all_types()
    }
}

const INCR_NONCE_AUTH_CODE: &str = "
    use.miden::account

    export.auth__basic
        push.1 exec.account::incr_nonce
    end
";

const NOOP_AUTH_CODE: &str = "
    use.miden::account

    export.auth__noop
        push.0 drop
    end
";

pub const ERR_WRONG_ARGS_MSG: &str = "auth procedure args are incorrect";

static CONDITIONAL_AUTH_CODE: LazyLock<String> = LazyLock::new(|| {
    format!(
        r#"
        use.miden::account

        const.WRONG_ARGS="{ERR_WRONG_ARGS_MSG}"

        export.auth__conditional
            # => [AUTH_ARGS]

            # If [97, 98, 99] is passed as an argument, all good.
            # Otherwise we error out.
            push.97 assert_eq.err=WRONG_ARGS
            push.98 assert_eq.err=WRONG_ARGS
            push.99 assert_eq.err=WRONG_ARGS

            # Last element is the incr_nonce_flag.
            if.true
                push.1 exec.account::incr_nonce
            end
            dropw dropw dropw dropw
        end
"#
    )
});

/// Creates a mock authentication [`AccountComponent`] for testing purposes.
///
/// The component defines an `auth__noop` procedure that does nothing (always succeeds).
pub struct NoopAuthComponent {
    library: Library,
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

/// TODO: Add documentation once #1501 is ready.
pub struct ConditionalAuthComponent {
    library: Library,
}

impl ConditionalAuthComponent {
    pub fn new(assembler: Assembler) -> Result<Self, AccountError> {
        let library = assembler
            .assemble_library([CONDITIONAL_AUTH_CODE.as_str()])
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

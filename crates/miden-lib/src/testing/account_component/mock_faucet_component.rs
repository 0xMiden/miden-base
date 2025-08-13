use miden_objects::account::{AccountCode, AccountComponent, AccountType};

use crate::testing::mock_account_code::MockAccountCodeExt;

// MOCK FAUCET COMPONENT
// ================================================================================================

/// A mock faucet account component for use in tests.
///
/// It uses the [`MockAccountCodeExt::mock_faucet_library`][faucet_lib] and contains no storage
/// slots.
///
/// This component supports the faucet [`AccountType`](miden_objects::account::AccountType)s for
/// testing purposes.
///
/// [faucet_lib]: crate::testing::mock_account_code::MockAccountCodeExt::mock_faucet_library
pub struct MockFaucetComponent;

impl From<MockFaucetComponent> for AccountComponent {
    fn from(_: MockFaucetComponent) -> Self {
        AccountComponent::new(AccountCode::mock_faucet_library(), vec![])
          .expect("mock faucet component should satisfy the requirements of a valid account component")
          .with_supported_type(AccountType::FungibleFaucet)
          .with_supported_type(AccountType::NonFungibleFaucet)
    }
}

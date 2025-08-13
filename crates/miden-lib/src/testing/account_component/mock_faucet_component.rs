use miden_objects::account::{AccountCode, AccountComponent, AccountType};

use crate::testing::mock_account_code::MockAccountCodeExt;

// MOCK FAUCET COMPONENT
// ================================================================================================

/// Creates a mock [`Library`](miden_objects::assembly::Library) which can be used to assemble
/// programs and as a library to create a mock [`AccountCode`](miden_objects::account::AccountCode)
/// interface. Transaction and note scripts that make use of this interface should be assembled with
/// this.
///
/// This component supports the faucet [`AccountType`](miden_objects::account::AccountType)s for
/// testing purposes.
pub struct MockFaucetComponent;

impl From<MockFaucetComponent> for AccountComponent {
    fn from(_: MockFaucetComponent) -> Self {
        AccountComponent::new(AccountCode::mock_faucet_library(), vec![])
          .expect("mock faucet component should satisfy the requirements of a valid account component")
          .with_supported_type(AccountType::FungibleFaucet)
          .with_supported_type(AccountType::NonFungibleFaucet)
    }
}

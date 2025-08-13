use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountComponent,
    AccountId,
    AccountStorage,
    AccountType,
    StorageMap,
    StorageSlot,
};
use miden_objects::asset::{AssetVault, NonFungibleAsset};
use miden_objects::testing::constants::{self};
use miden_objects::testing::noop_auth_component::NoopAuthComponent;
use miden_objects::testing::storage::FAUCET_STORAGE_DATA_SLOT;
use miden_objects::{Felt, Word, ZERO};

use crate::testing::account_component::{MockAccountComponent, MockFaucetComponent};

// MOCK ACCOUNT EXT
// ================================================================================================

/// Extension trait for [`Account`]s that return mocked accounts.
pub trait MockAccountExt {
    /// Creates an existing mock account with the provided auth component.
    fn mock(account_id: u128, auth: impl Into<AccountComponent>) -> Self;
    /// Creates a mock account with fungible faucet storage and the given account ID.
    fn mock_fungible_faucet(account_id: u128, initial_balance: Felt) -> Self;
    /// Creates a mock account with non-fungible faucet storage and the given account ID.
    fn mock_non_fungible_faucet(account_id: u128) -> Self;
}

impl MockAccountExt for Account {
    fn mock(account_id: u128, auth: impl Into<AccountComponent>) -> Self {
        let account_id = AccountId::try_from(account_id).unwrap();
        let account = AccountBuilder::new([1; 32])
            .account_type(account_id.account_type())
            .with_auth_component(auth)
            .with_component(MockAccountComponent::with_slots(AccountStorage::mock_storage_slots()))
            .with_assets(AssetVault::mock().assets())
            .build_existing()
            .expect("account should be valid");
        let (_id, vault, storage, code, nonce) = account.into_parts();

        Account::from_parts(account_id, vault, storage, code, nonce)
    }

    fn mock_fungible_faucet(account_id: u128, initial_balance: Felt) -> Self {
        let account_id = AccountId::try_from(account_id).unwrap();
        assert_eq!(account_id.account_type(), AccountType::FungibleFaucet);

        let account = AccountBuilder::new([1; 32])
            .account_type(account_id.account_type())
            .with_auth_component(NoopAuthComponent)
            .with_component(MockFaucetComponent)
            .build_existing()
            .expect("account should be valid");
        let (_id, vault, mut storage, code, nonce) = account.into_parts();

        let faucet_data_slot = Word::from([ZERO, ZERO, ZERO, initial_balance]);
        storage.set_item(FAUCET_STORAGE_DATA_SLOT, faucet_data_slot).unwrap();

        Account::from_parts(account_id, vault, storage, code, nonce)
    }

    fn mock_non_fungible_faucet(account_id: u128) -> Self {
        let account_id = AccountId::try_from(account_id).unwrap();
        assert_eq!(account_id.account_type(), AccountType::NonFungibleFaucet);

        let account = AccountBuilder::new([1; 32])
            .account_type(account_id.account_type())
            .with_auth_component(NoopAuthComponent)
            .with_component(MockFaucetComponent)
            .build_existing()
            .expect("account should be valid");
        let (_id, vault, _storage, code, nonce) = account.into_parts();

        let asset = NonFungibleAsset::mock(&constants::NON_FUNGIBLE_ASSET_DATA_2);
        let non_fungible_storage_map =
            StorageMap::with_entries([(asset.vault_key(), asset.into())]).unwrap();
        let storage =
            AccountStorage::new(vec![StorageSlot::Map(non_fungible_storage_map)]).unwrap();

        Account::from_parts(account_id, vault, storage, code, nonce)
    }
}

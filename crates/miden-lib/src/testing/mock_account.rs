use miden_objects::account::{
    Account,
    AccountBuilder,
    AccountComponent,
    AccountId,
    AccountStorage,
    StorageMap,
    StorageSlot,
};
use miden_objects::asset::{AssetVault, NonFungibleAsset};
use miden_objects::testing::constants::{self};
use miden_objects::testing::noop_auth_component::NoopAuthComponent;
use miden_objects::testing::storage::FAUCET_STORAGE_DATA_SLOT;
use miden_objects::{Felt, Word, ZERO};

use crate::testing::account_component::AccountMockComponent;

// MOCK ACCOUNT EXT
// ================================================================================================

/// Extension trait for [`Account`]s that return mocked accounts.
pub trait MockAccountExt {
    /// Creates an existing mock account with the provided auth component.
    fn mock(account_id: u128, auth: impl Into<AccountComponent>) -> Self;
    /// Creates a mock account with fungible faucet storage and the given account ID.
    fn mock_fungible_faucet(account_id: u128, nonce: Felt, initial_balance: Felt) -> Self;
    /// Creates a mock account with non-fungible faucet storage and the given account ID.
    fn mock_non_fungible_faucet(account_id: u128, nonce: Felt, empty_reserved_slot: bool) -> Self;
}

impl MockAccountExt for Account {
    fn mock(account_id: u128, auth: impl Into<AccountComponent>) -> Self {
        let account_id = AccountId::try_from(account_id).unwrap();
        let mock_component =
            AccountMockComponent::new_with_slots(AccountStorage::mock_storage_slots()).unwrap();
        let account = AccountBuilder::new([1; 32])
            .account_type(account_id.account_type())
            .with_auth_component(auth)
            .with_component(mock_component)
            .with_assets(AssetVault::mock().assets())
            .build_existing()
            .expect("account should be valid");
        let (_id, vault, storage, code, nonce) = account.into_parts();

        Account::from_parts(account_id, vault, storage, code, nonce)
    }

    fn mock_fungible_faucet(account_id: u128, nonce: Felt, initial_balance: Felt) -> Self {
        let account_id = AccountId::try_from(account_id).unwrap();

        let account = AccountBuilder::new([1; 32])
            .account_type(account_id.account_type())
            .with_auth_component(NoopAuthComponent)
            .with_component(AccountMockComponent::new_with_empty_slots().unwrap())
            .build_existing()
            .expect("account should be valid");
        let (_id, vault, mut storage, code, _nonce) = account.into_parts();

        let faucet_data_slot = Word::from([ZERO, ZERO, ZERO, initial_balance]);
        storage.set_item(FAUCET_STORAGE_DATA_SLOT, faucet_data_slot).unwrap();

        Account::from_parts(account_id, vault, storage, code, nonce)
    }

    fn mock_non_fungible_faucet(account_id: u128, nonce: Felt, empty_reserved_slot: bool) -> Self {
        let entries = match empty_reserved_slot {
            true => {
                vec![]
            },
            false => {
                let asset = NonFungibleAsset::mock(&constants::NON_FUNGIBLE_ASSET_DATA_2);
                let vault_key = asset.vault_key();
                vec![(vault_key, asset.into())]
            },
        };

        // construct nft tree
        let nft_storage_map = StorageMap::with_entries(entries).unwrap();

        // The component does not have any storage slots so we don't need to instantiate storage
        // from the component. We also need to set the custom value for the storage map so we
        // construct storage manually.
        let storage = AccountStorage::new(vec![StorageSlot::Map(nft_storage_map)]).unwrap();

        let account_id = AccountId::try_from(account_id).unwrap();

        let account = AccountBuilder::new([1; 32])
            .account_type(account_id.account_type())
            .with_auth_component(NoopAuthComponent)
            .with_component(AccountMockComponent::new_with_empty_slots().unwrap())
            .build_existing()
            .expect("account should be valid");
        let (_id, vault, _storage, code, _nonce) = account.into_parts();

        Account::from_parts(account_id, vault, storage, code, nonce)
    }
}

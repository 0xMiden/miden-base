use assembly::Assembler;
use vm_core::FieldElement;

use super::constants::{self, FUNGIBLE_ASSET_AMOUNT, NON_FUNGIBLE_ASSET_DATA};
use crate::{
    Felt, Word, ZERO,
    account::{
        Account, AccountCode, AccountComponent, AccountId, AccountStorage, StorageMap, StorageSlot,
    },
    asset::{Asset, AssetVault, FungibleAsset, NonFungibleAsset},
    testing::{
        account_component::{AccountMockComponent, NoopAuthComponent},
        account_id::{
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1,
            ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2,
        },
        storage::FAUCET_STORAGE_DATA_SLOT,
    },
};

// MOCK ACCOUNT
// ================================================================================================

impl Account {
    /// Creates a non-new mock account with a defined number of assets and storage
    pub fn mock(
        account_id: u128,
        nonce: Felt,
        auth: impl Into<AccountComponent>,
        assembler: Assembler,
    ) -> Self {
        let account_vault = if nonce == Felt::ZERO {
            AssetVault::default()
        } else {
            AssetVault::mock()
        };

        let account_id = AccountId::try_from(account_id).unwrap();
        let mock_component =
            AccountMockComponent::new_with_slots(assembler, AccountStorage::mock_storage_slots())
                .unwrap();
        let (account_code, account_storage) = Account::initialize_from_components(
            account_id.account_type(),
            &[auth.into(), mock_component.into()],
        )
        .unwrap();

        Account::from_parts(account_id, account_vault, account_storage, account_code, nonce)
    }

    pub fn mock_fungible_faucet(
        account_id: u128,
        nonce: Felt,
        initial_balance: Felt,
        assembler: Assembler,
    ) -> Self {
        let account_id = AccountId::try_from(account_id).unwrap();

        let mock_component = AccountMockComponent::new_with_empty_slots(assembler.clone()).unwrap();

        let auth_component: AccountComponent = NoopAuthComponent::new(assembler).unwrap().into();

        let (account_code, mut account_storage) = Account::initialize_from_components(
            account_id.account_type(),
            &[auth_component, mock_component.into()],
        )
        .unwrap();

        let faucet_data_slot = Word::from([ZERO, ZERO, ZERO, initial_balance]);
        account_storage.set_item(FAUCET_STORAGE_DATA_SLOT, faucet_data_slot).unwrap();

        Account::from_parts(account_id, AssetVault::default(), account_storage, account_code, nonce)
    }

    pub fn mock_non_fungible_faucet(
        account_id: u128,
        nonce: Felt,
        empty_reserved_slot: bool,
        assembler: Assembler,
    ) -> Self {
        let entries = match empty_reserved_slot {
            true => vec![],
            false => {
                let asset = NonFungibleAsset::mock(&constants::NON_FUNGIBLE_ASSET_DATA_2);
                let vault_key = asset.vault_key();
                vec![(vault_key, asset.into())]
            },
        };

        // construct nft tree
        let nft_storage_map = StorageMap::with_entries(entries).unwrap();

        let account_id = AccountId::try_from(account_id).unwrap();

        let auth_component: AccountComponent =
            NoopAuthComponent::new(assembler.clone()).unwrap().into();

        let mock_component = AccountMockComponent::new_with_empty_slots(assembler).unwrap();

        let account_code = AccountCode::from_components(
            &[auth_component, mock_component.into()],
            account_id.account_type(),
        )
        .unwrap();

        // The component does not have any storage slots so we don't need to instantiate storage
        // from the component. We also need to set the custom value for the storage map so we
        // construct storage manually.
        let account_storage = AccountStorage::new(vec![StorageSlot::Map(nft_storage_map)]).unwrap();

        Account::from_parts(account_id, AssetVault::default(), account_storage, account_code, nonce)
    }
}

impl AssetVault {
    /// Creates an [AssetVault] with 4 default assets.
    ///
    /// The ids of the assets added to the vault are defined by the following constants:
    ///
    /// - ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET
    /// - ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1
    /// - ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2
    /// - ACCOUNT_ID_PUBLIC_NON_FUNGIBLE_FAUCET
    pub fn mock() -> Self {
        let faucet_id: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap();
        let fungible_asset =
            Asset::Fungible(FungibleAsset::new(faucet_id, FUNGIBLE_ASSET_AMOUNT).unwrap());

        let faucet_id_1: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_1.try_into().unwrap();
        let fungible_asset_1 =
            Asset::Fungible(FungibleAsset::new(faucet_id_1, FUNGIBLE_ASSET_AMOUNT).unwrap());

        let faucet_id_2: AccountId = ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET_2.try_into().unwrap();
        let fungible_asset_2 =
            Asset::Fungible(FungibleAsset::new(faucet_id_2, FUNGIBLE_ASSET_AMOUNT).unwrap());

        let non_fungible_asset = NonFungibleAsset::mock(&NON_FUNGIBLE_ASSET_DATA);
        AssetVault::new(&[fungible_asset, fungible_asset_1, fungible_asset_2, non_fungible_asset])
            .unwrap()
    }
}

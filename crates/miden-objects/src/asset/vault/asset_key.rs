use core::fmt;

use crate::account::AccountId;
use crate::account::AccountType::{
    FungibleFaucet,
    NonFungibleFaucet,
    RegularAccountImmutableCode,
    RegularAccountUpdatableCode,
};
use crate::asset::{Asset, FungibleAsset, NonFungibleAsset};
use crate::{AccountIdError, Word};

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct AssetKey(Word);

impl AssetKey {
    pub fn inner(&self) -> Word {
        self.0
    }

    pub fn from_account_id(account_id: AccountId) -> Result<Self, AccountIdError> {
        match account_id.account_type() {
            FungibleFaucet => Ok(Self(FungibleAsset::vault_key_from_faucet(account_id))),
            NonFungibleFaucet => !todo!(),
            RegularAccountImmutableCode | RegularAccountUpdatableCode => {
                Err(AccountIdError::UnknownAssetKey)
            },
        }
    }
}

impl fmt::Display for AssetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner())
    }
}

impl Into<AssetKey> for Word {
    fn into(self) -> AssetKey {
        AssetKey(self)
    }
}

// CONVERSIONS FROM ASSET
// ================================================================================================

impl From<Asset> for AssetKey {
    fn from(asset: Asset) -> Self {
        asset.vault_key()
    }
}

// CONVERSIONS FROM FUNGIBLE ASSET
// ================================================================================================

impl From<FungibleAsset> for AssetKey {
    fn from(fungible_asset: FungibleAsset) -> Self {
        fungible_asset.vault_key()
    }
}

// CONVERSIONS FROM NON-FUNGIBLE ASSET
// ================================================================================================

impl From<NonFungibleAsset> for AssetKey {
    fn from(non_fungible_asset: NonFungibleAsset) -> Self {
        non_fungible_asset.vault_key()
    }
}

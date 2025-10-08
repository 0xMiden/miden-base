use core::fmt;

use crate::Word;
use crate::account::AccountType::FungibleFaucet;
use crate::account::{AccountId, AccountIdPrefix};
use crate::asset::{Asset, FungibleAsset, NonFungibleAsset};

/// The key of an [`Asset`] in the asset vault.
///
/// The layout of an asset key is:
/// - Fungible asset key: `[0, 0, faucet_id_suffix, faucet_id_prefix]`.
/// - Non-fungible asset key: `[faucet_id_prefix, hash1, hash2, hash0']`, where `hash0'` is
///   equivalent to `hash0` with the fungible bit set to `0`. See [`NonFungibleAsset::vault_key`]
///   for more details.
///
/// For details on the layout of an asset, see the documentation of [`Asset`].
///
/// ## Guarantees
///
/// This type guarantees that it contains a valid fungible or non-fungible asset key:
/// - For fungible assets
///   - The felt at index 3 has the fungible bit set to 1 and it is a valid account ID prefix.
///   - The felt at index 2 is a valid account ID suffix.
/// - For non-fungible assets
///   - The felt at index 3 has the fungible bit set to 0.
///   - The felt at index 0 is a valid account ID prefix.
///
/// The fungible bit is the bit in the [`AccountId`] that encodes whether the ID is a faucet.
#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct AssetKey(Word);

impl AssetKey {
    pub fn new_unchecked(value: Word) -> Self {
        Self(value)
    }

    pub fn as_word(&self) -> Word {
        self.0
    }

    /// Returns an [`AccountIdPrefix`] from the asset key.
    ///
    /// # Warning
    ///
    /// Validity of the ID prefix must be ensured by the caller. An invalid ID may lead to panics.
    pub fn faucet_id_prefix(&self) -> AccountIdPrefix {
        if self.as_word()[0].as_int() == 0 && self.as_word()[1].as_int() == 0 {
            AccountIdPrefix::new_unchecked(self.as_word()[3])
        } else {
            AccountIdPrefix::new_unchecked(self.as_word()[0])
        }
    }

    /// Returns an [`AccountId`] from the asset key.
    ///
    /// # Warning
    ///
    /// Validity of the ID prefix must be ensured by the caller. An invalid ID may lead to panics.
    /// This works only for fungible assets.
    pub fn faucet_id(&self) -> Option<AccountId> {
        if self.as_word()[0].as_int() == 0 && self.as_word()[1].as_int() == 0 {
            Some(AccountId::new_unchecked([self.as_word()[3], self.as_word()[2]]))
        } else {
            None
        }
    }

    /// Constructs a fungible asset's key from a faucet ID.
    ///
    /// Returns `None` if the provided ID is not of type
    /// [`AccountType::FungibleFaucet`](crate::account::AccountType::FungibleFaucet)
    pub fn from_account_id(faucet_id: AccountId) -> Option<Self> {
        match faucet_id.account_type() {
            FungibleFaucet => Some(Self(FungibleAsset::vault_key_from_faucet(faucet_id))),
            _ => None,
        }
    }
}

impl fmt::Display for AssetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_word())
    }
}
// CONVERSIONS
// ================================================================================================

impl From<AssetKey> for Word {
    fn from(val: AssetKey) -> Self {
        val.0
    }
}

impl From<Word> for AssetKey {
    fn from(val: Word) -> Self {
        AssetKey(val)
    }
}

impl From<Asset> for AssetKey {
    fn from(asset: Asset) -> Self {
        asset.vault_key()
    }
}

impl From<FungibleAsset> for AssetKey {
    fn from(fungible_asset: FungibleAsset) -> Self {
        fungible_asset.vault_key()
    }
}

impl From<NonFungibleAsset> for AssetKey {
    fn from(non_fungible_asset: NonFungibleAsset) -> Self {
        non_fungible_asset.vault_key()
    }
}

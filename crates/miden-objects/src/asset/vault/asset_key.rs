use core::fmt;

use miden_core::Felt;

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
    /// Creates a new [`AssetKey`] from the given [`Word`] **without performing validation**.
    ///
    /// ## Warning
    ///
    /// This function **does not check** whether the provided `Word` represents a valid
    /// fungible or non-fungible asset key.
    pub fn new_unchecked(value: Word) -> Self {
        Self(value)
    }

    /// Returns the underlying [`Word`] that makes up this [`AssetKey`].
    pub fn as_word(&self) -> Word {
        self.0
    }

    /// Returns an [`AccountIdPrefix`] from the asset key.
    pub fn faucet_id_prefix(&self) -> AccountIdPrefix {
        if self.is_fungible() {
            AccountIdPrefix::new_unchecked(self.as_word()[3])
        } else {
            AccountIdPrefix::new_unchecked(self.as_word()[0])
        }
    }

    /// Returns the [`AccountId`] from the asset key if it is a fungible asset, `None` otherwise.
    pub fn faucet_id(&self) -> Option<AccountId> {
        if self.is_fungible() {
            Some(AccountId::new_unchecked([self.as_word()[3], self.as_word()[2]]))
        } else {
            None
        }
    }

    // TODO: Replace with https://github.com/0xMiden/crypto/issues/515 once implemented.
    /// Returns the leaf index of a vault key.
    pub fn to_leaf_index(&self) -> Felt {
        // The third element in an SMT key is the index.
        self.as_word()[3]
    }

    /// Returns `true` if the asset key is for a fungible asset, `false` otherwise.
    fn is_fungible(&self) -> bool {
        self.as_word()[0].as_int() == 0 && self.as_word()[1].as_int() == 0
    }

    /// Constructs a fungible asset's key from a faucet ID.
    ///
    /// Returns `None` if the provided ID is not of type
    /// [`AccountType::FungibleFaucet`](crate::account::AccountType::FungibleFaucet)
    pub fn from_account_id(faucet_id: AccountId) -> Option<Self> {
        match faucet_id.account_type() {
            FungibleFaucet => {
                let mut key = Word::empty();
                key[2] = faucet_id.suffix();
                key[3] = faucet_id.prefix().as_felt();
                Some(AssetKey::new_unchecked(key))
            },
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

#[cfg(test)]
mod tests {
    use miden_core::Felt;

    use super::*;
    use crate::account::{AccountIdV0, AccountStorageMode, AccountType};

    fn make_fungible_key(prefix: u64, suffix: u64) -> AssetKey {
        let word = [Felt::new(0), Felt::new(0), Felt::new(suffix), Felt::new(prefix)].into();
        AssetKey::new_unchecked(word)
    }

    fn make_non_fungible_key(prefix: u64) -> AssetKey {
        let word = [Felt::new(prefix), Felt::new(11), Felt::new(22), Felt::new(33)].into();
        AssetKey::new_unchecked(word)
    }

    #[test]
    fn test_faucet_id_prefix_for_fungible_asset() {
        let input = [0xff; 15];

        for storage_mode in [AccountStorageMode::Private, AccountStorageMode::Public] {
            let id = AccountIdV0::dummy(input, AccountType::FungibleFaucet, storage_mode);

            let key =
                AssetKey::from_account_id(id.into()).expect("Expected AssetKey for FungibleFaucet");

            // faucet_id_prefix() should match AccountId prefix
            assert_eq!(key.faucet_id_prefix().as_felt(), id.prefix().as_felt());

            // faucet_id() should return the same account id
            let faucet_id = key.faucet_id().expect("Expected Some(AccountId)");
            assert_eq!(faucet_id.prefix().as_felt(), id.prefix().as_felt());
            assert_eq!(faucet_id.suffix(), id.suffix());
        }
    }

    #[test]
    fn test_faucet_id_prefix_for_non_fungible_asset() {
        let prefix_val = 0;
        let key = make_non_fungible_key(prefix_val);

        let prefix = key.faucet_id_prefix();
        assert_eq!(prefix.as_felt(), Felt::new(prefix_val));
    }

    #[test]
    fn test_faucet_id_for_fungible_asset() {
        let prefix_val = 0;
        let suffix_val = 0;
        let key = make_fungible_key(prefix_val, suffix_val);

        let faucet_id = key.faucet_id().expect("Expected Some(AccountId)");
        assert_eq!(faucet_id.prefix().as_felt(), Felt::new(prefix_val));
        assert_eq!(faucet_id.suffix(), Felt::new(suffix_val));
    }

    #[test]
    fn test_faucet_id_for_non_fungible_asset() {
        let input = [0xff; 15];

        for storage_mode in [AccountStorageMode::Private, AccountStorageMode::Public] {
            let id = AccountIdV0::dummy(input, AccountType::NonFungibleFaucet, storage_mode);

            assert!(AssetKey::from_account_id(id.into()).is_none());
        }
    }
}

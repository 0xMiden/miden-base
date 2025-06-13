use alloc::string::ToString;

use super::{
    Account, ByteReader, ByteWriter, Deserializable, DeserializationError, Felt, Serializable,
    Word, ZERO,
};
use crate::AccountDeltaError;

mod storage;
pub use storage::{AccountStorageDelta, StorageMapDelta};

mod vault;
pub use vault::{
    AccountVaultDelta, FungibleAssetDelta, NonFungibleAssetDelta, NonFungibleDeltaAction,
};

// ACCOUNT DELTA
// ================================================================================================

/// [AccountDelta] stores the differences between two account states.
///
/// The differences are represented as follows:
/// - storage: an [AccountStorageDelta] that contains the changes to the account storage.
/// - vault: an [AccountVaultDelta] object that contains the changes to the account vault.
/// - nonce: if the nonce of the account has changed, the _delta_ of the nonce is stored, i.e. the
///   value by which the nonce increased.
///
/// TODO: add ability to trace account code updates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountDelta {
    storage: AccountStorageDelta,
    vault: AccountVaultDelta,
    nonce: Option<Felt>,
}

impl AccountDelta {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    /// Returns new [AccountDelta] instantiated from the provided components.
    ///
    /// # Errors
    ///
    /// - Returns an error if storage or vault were updated, but the nonce was either not updated or
    ///   set to 0.
    pub fn new(
        storage: AccountStorageDelta,
        vault: AccountVaultDelta,
        nonce: Option<Felt>,
    ) -> Result<Self, AccountDeltaError> {
        // nonce must be updated if either account storage or vault were updated
        validate_nonce(nonce, &storage, &vault)?;

        Ok(Self { storage, vault, nonce })
    }

    /// Merge another [AccountDelta] into this one.
    pub fn merge(&mut self, other: Self) -> Result<(), AccountDeltaError> {
        self.nonce = match (self.nonce, other.nonce) {
            (Some(self_nonce_delta), Some(other_nonce_delta)) => {
                Some(self_nonce_delta + other_nonce_delta)
            },
            (self_nonce_delta, other_nonce_delta) => other_nonce_delta.or(self_nonce_delta),
        };

        self.storage.merge(other.storage)?;
        self.vault.merge(other.vault)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns true if this account delta does not contain any updates.
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty() && self.vault.is_empty()
    }

    /// Returns storage updates for this account delta.
    pub fn storage(&self) -> &AccountStorageDelta {
        &self.storage
    }

    /// Returns vault updates for this account delta.
    pub fn vault(&self) -> &AccountVaultDelta {
        &self.vault
    }

    /// Returns the amount by which the nonce changed.
    pub fn nonce(&self) -> Option<Felt> {
        self.nonce
    }

    /// Converts this storage delta into individual delta components.
    pub fn into_parts(self) -> (AccountStorageDelta, AccountVaultDelta, Option<Felt>) {
        (self.storage, self.vault, self.nonce)
    }
}

/// Describes the details of an account state transition resulting from applying a transaction to
/// the account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccountUpdateDetails {
    /// Account is private (no on-chain state change).
    Private,

    /// The whole state is needed for new accounts.
    New(Account),

    /// For existing accounts, only the delta is needed.
    Delta(AccountDelta),
}

impl AccountUpdateDetails {
    /// Returns `true` if the account update details are for private account.
    pub fn is_private(&self) -> bool {
        matches!(self, Self::Private)
    }

    /// Merges the `other` update into this one.
    ///
    /// This account update is assumed to come before the other.
    pub fn merge(self, other: AccountUpdateDetails) -> Result<Self, AccountDeltaError> {
        let merged_update = match (self, other) {
            (AccountUpdateDetails::Private, AccountUpdateDetails::Private) => {
                AccountUpdateDetails::Private
            },
            (AccountUpdateDetails::New(mut account), AccountUpdateDetails::Delta(delta)) => {
                account.apply_delta(&delta).map_err(|err| {
                    AccountDeltaError::AccountDeltaApplicationFailed {
                        account_id: account.id(),
                        source: err,
                    }
                })?;

                AccountUpdateDetails::New(account)
            },
            (AccountUpdateDetails::Delta(mut delta), AccountUpdateDetails::Delta(new_delta)) => {
                delta.merge(new_delta)?;
                AccountUpdateDetails::Delta(delta)
            },
            (left, right) => {
                return Err(AccountDeltaError::IncompatibleAccountUpdates {
                    left_update_type: left.as_tag_str(),
                    right_update_type: right.as_tag_str(),
                });
            },
        };

        Ok(merged_update)
    }

    /// Returns the tag of the [`AccountUpdateDetails`] as a string for inclusion in error messages.
    pub(crate) const fn as_tag_str(&self) -> &'static str {
        match self {
            AccountUpdateDetails::Private => "private",
            AccountUpdateDetails::New(_) => "new",
            AccountUpdateDetails::Delta(_) => "delta",
        }
    }
}

/// Converts an [Account] into an [AccountDelta] for initial delta construction.
impl From<Account> for AccountDelta {
    fn from(account: Account) -> Self {
        let (_id, vault, storage, _code, nonce) = account.into_parts();
        AccountDelta {
            storage: storage.into(),
            vault: (&vault).into(),
            nonce: Some(nonce),
        }
    }
}

// SERIALIZATION
// ================================================================================================

impl Serializable for AccountDelta {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.storage.write_into(target);
        self.vault.write_into(target);
        self.nonce.write_into(target);
    }

    fn get_size_hint(&self) -> usize {
        self.storage.get_size_hint() + self.vault.get_size_hint() + self.nonce.get_size_hint()
    }
}

impl Deserializable for AccountDelta {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let storage = AccountStorageDelta::read_from(source)?;
        let vault = AccountVaultDelta::read_from(source)?;
        let nonce = <Option<Felt>>::read_from(source)?;

        validate_nonce(nonce, &storage, &vault)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))?;

        Ok(Self { storage, vault, nonce })
    }
}

impl Serializable for AccountUpdateDetails {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        match self {
            AccountUpdateDetails::Private => {
                0_u8.write_into(target);
            },
            AccountUpdateDetails::New(account) => {
                1_u8.write_into(target);
                account.write_into(target);
            },
            AccountUpdateDetails::Delta(delta) => {
                2_u8.write_into(target);
                delta.write_into(target);
            },
        }
    }

    fn get_size_hint(&self) -> usize {
        // Size of the serialized enum tag.
        let u8_size = 0u8.get_size_hint();

        match self {
            AccountUpdateDetails::Private => u8_size,
            AccountUpdateDetails::New(account) => u8_size + account.get_size_hint(),
            AccountUpdateDetails::Delta(account_delta) => u8_size + account_delta.get_size_hint(),
        }
    }
}

impl Deserializable for AccountUpdateDetails {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        match u8::read_from(source)? {
            0 => Ok(Self::Private),
            1 => Ok(Self::New(Account::read_from(source)?)),
            2 => Ok(Self::Delta(AccountDelta::read_from(source)?)),
            v => Err(DeserializationError::InvalidValue(format!(
                "Unknown variant {v} for AccountDetails"
            ))),
        }
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Checks if the nonce was updated correctly given the provided storage and vault deltas.
///
/// # Errors
/// Returns an error if storage or vault were updated, but the nonce was either not updated
/// or set to 0.
fn validate_nonce(
    nonce: Option<Felt>,
    storage: &AccountStorageDelta,
    vault: &AccountVaultDelta,
) -> Result<(), AccountDeltaError> {
    if !storage.is_empty() || !vault.is_empty() {
        match nonce {
            Some(nonce) => {
                if nonce == ZERO {
                    return Err(AccountDeltaError::InconsistentNonceUpdate(
                        "zero nonce for a non-empty account delta".to_string(),
                    ));
                }
            },
            None => {
                return Err(AccountDeltaError::InconsistentNonceUpdate(
                    "nonce not updated for non-empty account delta".to_string(),
                ));
            },
        }
    }

    Ok(())
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {

    use vm_core::{Felt, FieldElement, utils::Serializable};

    use super::{AccountDelta, AccountStorageDelta, AccountVaultDelta};
    use crate::{
        ONE, ZERO,
        account::{
            Account, AccountCode, AccountId, AccountStorage, AccountStorageMode, AccountType,
            StorageMapDelta, delta::AccountUpdateDetails,
        },
        asset::{Asset, AssetVault, FungibleAsset, NonFungibleAsset, NonFungibleAssetDetails},
        testing::account_id::{
            ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE, AccountIdBuilder,
        },
    };

    #[test]
    fn account_delta_nonce_validation() {
        // empty delta
        let storage_delta = AccountStorageDelta::default();
        let vault_delta = AccountVaultDelta::default();

        assert!(AccountDelta::new(storage_delta.clone(), vault_delta.clone(), None).is_ok());
        assert!(AccountDelta::new(storage_delta.clone(), vault_delta.clone(), Some(ONE)).is_ok());

        // non-empty delta
        let storage_delta = AccountStorageDelta::from_iters([1], [], []);

        assert!(AccountDelta::new(storage_delta.clone(), vault_delta.clone(), None).is_err());
        assert!(AccountDelta::new(storage_delta.clone(), vault_delta.clone(), Some(ZERO)).is_err());
        assert!(AccountDelta::new(storage_delta.clone(), vault_delta.clone(), Some(ONE)).is_ok());
    }

    #[test]
    fn account_update_details_size_hint() {
        // AccountDelta

        let storage_delta = AccountStorageDelta::default();
        let vault_delta = AccountVaultDelta::default();
        assert_eq!(storage_delta.to_bytes().len(), storage_delta.get_size_hint());
        assert_eq!(vault_delta.to_bytes().len(), vault_delta.get_size_hint());

        let account_delta = AccountDelta::new(storage_delta, vault_delta, None).unwrap();
        assert_eq!(account_delta.to_bytes().len(), account_delta.get_size_hint());

        let storage_delta = AccountStorageDelta::from_iters(
            [1],
            [(2, [ONE, ONE, ONE, ONE]), (3, [ONE, ONE, ZERO, ONE])],
            [(
                4,
                StorageMapDelta::from_iters(
                    [[ONE, ONE, ONE, ZERO], [ZERO, ONE, ONE, ONE]],
                    [([ONE, ONE, ONE, ONE], [ONE, ONE, ONE, ONE])],
                ),
            )],
        );

        let non_fungible: Asset = NonFungibleAsset::new(
            &NonFungibleAssetDetails::new(
                AccountIdBuilder::new()
                    .account_type(AccountType::NonFungibleFaucet)
                    .storage_mode(AccountStorageMode::Public)
                    .build_with_rng(&mut rand::rng())
                    .prefix(),
                vec![6],
            )
            .unwrap(),
        )
        .unwrap()
        .into();
        let fungible_2: Asset = FungibleAsset::new(
            AccountIdBuilder::new()
                .account_type(AccountType::FungibleFaucet)
                .storage_mode(AccountStorageMode::Public)
                .build_with_rng(&mut rand::rng()),
            10,
        )
        .unwrap()
        .into();
        let vault_delta = AccountVaultDelta::from_iters([non_fungible], [fungible_2]);

        assert_eq!(storage_delta.to_bytes().len(), storage_delta.get_size_hint());
        assert_eq!(vault_delta.to_bytes().len(), vault_delta.get_size_hint());

        let account_delta = AccountDelta::new(storage_delta, vault_delta, Some(ONE)).unwrap();
        assert_eq!(account_delta.to_bytes().len(), account_delta.get_size_hint());

        // Account

        let account_id =
            AccountId::try_from(ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE).unwrap();

        let asset_vault = AssetVault::mock();
        assert_eq!(asset_vault.to_bytes().len(), asset_vault.get_size_hint());

        let account_storage = AccountStorage::mock();
        assert_eq!(account_storage.to_bytes().len(), account_storage.get_size_hint());

        let account_code = AccountCode::mock();
        assert_eq!(account_code.to_bytes().len(), account_code.get_size_hint());

        let account =
            Account::from_parts(account_id, asset_vault, account_storage, account_code, Felt::ZERO);
        assert_eq!(account.to_bytes().len(), account.get_size_hint());

        // AccountUpdateDetails

        let update_details_private = AccountUpdateDetails::Private;
        assert_eq!(update_details_private.to_bytes().len(), update_details_private.get_size_hint());

        let update_details_delta = AccountUpdateDetails::Delta(account_delta);
        assert_eq!(update_details_delta.to_bytes().len(), update_details_delta.get_size_hint());

        let update_details_new = AccountUpdateDetails::New(account);
        assert_eq!(update_details_new.to_bytes().len(), update_details_new.get_size_hint());
    }
}

use alloc::{string::ToString, vec::Vec};

use crate::{
    AccountDeltaError, Felt, Word, ZERO,
    account::{Account, AccountId},
    crypto::SequentialCommit,
    utils::{ByteReader, ByteWriter, Deserializable, DeserializationError, Serializable},
};

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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountDelta {
    /// The ID of the account to which this delta applies. If the delta is created during
    /// transaction execution, that is the native account of the transaction.
    account_id: AccountId,
    /// The delta of the account's storage.
    storage: AccountStorageDelta,
    /// The delta of the account's asset vault.
    vault: AccountVaultDelta,
    /// The value by which the nonce was incremented. Must be greater than zero if storage or vault
    /// are non-empty.
    nonce_delta: Felt,
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
        account_id: AccountId,
        storage: AccountStorageDelta,
        vault: AccountVaultDelta,
        nonce_delta: Felt,
    ) -> Result<Self, AccountDeltaError> {
        // nonce must be updated if either account storage or vault were updated
        validate_nonce(nonce_delta, &storage, &vault)?;

        Ok(Self { account_id, storage, vault, nonce_delta })
    }

    /// Merge another [AccountDelta] into this one.
    pub fn merge(&mut self, other: Self) -> Result<(), AccountDeltaError> {
        let new_nonce_delta = self.nonce_delta + other.nonce_delta;

        if new_nonce_delta.as_int() < self.nonce_delta.as_int() {
            return Err(AccountDeltaError::NonceIncrementOverflow {
                current: self.nonce_delta,
                increment: other.nonce_delta,
                new: new_nonce_delta,
            });
        }

        self.nonce_delta = new_nonce_delta;

        self.storage.merge(other.storage)?;
        self.vault.merge(other.vault)
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns true if this account delta does not contain any vault, storage or nonce updates.
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty() && self.vault.is_empty() && self.nonce_delta == ZERO
    }

    /// Returns storage updates for this account delta.
    pub fn storage(&self) -> &AccountStorageDelta {
        &self.storage
    }

    /// Returns vault updates for this account delta.
    pub fn vault(&self) -> &AccountVaultDelta {
        &self.vault
    }

    /// Returns the amount by which the nonce was incremented.
    pub fn nonce_delta(&self) -> Felt {
        self.nonce_delta
    }

    /// Returns the account ID to which this delta applies.
    pub fn id(&self) -> AccountId {
        self.account_id
    }

    /// Converts this storage delta into individual delta components.
    pub fn into_parts(self) -> (AccountStorageDelta, AccountVaultDelta, Felt) {
        (self.storage, self.vault, self.nonce_delta)
    }

    /// Computes the commitment to the account delta.
    ///
    /// The delta is a sequential hash over a vector of field elements which starts out empty and
    /// is appended to in the following way. Whenever sorting is expected, it is that of a link map
    /// key. The WORD layout is in memory-order.
    ///
    /// - Append `[[nonce_delta, 0, account_id_suffix, account_id_prefix], EMPTY_WORD]`, where
    ///   account_id_{prefix,suffix} are the prefix and suffix felts of the native account id and
    ///   nonce_delta is the value by which the nonce was incremented.
    /// - Fungible Asset Delta
    ///   - For each **updated** fungible asset, sorted by its vault key, whose amount delta is
    ///     **non-zero**:
    ///     - Append `[domain = 1, was_added, 0, 0]`.
    ///     - Append `[amount, 0, faucet_id_suffix, faucet_id_prefix]` where amount is the delta by
    ///       which the fungible asset's amount has changed and was_added is a boolean flag
    ///       indicating whether the amount was added (1) or subtracted (0).
    /// - Non-Fungible Asset Delta
    ///   - For each **updated** non-fungible asset, sorted by its vault key:
    ///     - Append `[domain = 1, was_added, 0, 0]` where was_added is a boolean flag indicating
    ///       whether the asset was added (1) or removed (0). Note that the domain is the same for
    ///       assets since `faucet_id_prefix` is at the same position in the layout for both assets,
    ///       and, by design, it is never the same for fungible and non-fungible assets.
    ///     - Append `[hash0, hash1, hash2, faucet_id_prefix]`, i.e. the non-fungible asset.
    /// - Storage Slots - for each slot **whose value has changed**, depending on the slot type:
    ///   - Value Slot
    ///     - Append `[[domain = 2, slot_idx, 0, 0], NEW_VALUE]` where NEW_VALUE is the new value of
    ///       the slot and slot_idx is the index of the slot.
    ///   - Map Slot
    ///     - For each key-value pair, sorted by key, whose new value is different from the previous
    ///       value in the map:
    ///       - Append `[KEY, NEW_VALUE]`.
    ///     - Append `[[domain = 3, slot_idx, num_changed_entries, 0], 0, 0, 0, 0]`, except if
    ///       `num_changed_entries` is 0, where slot_idx is the index of the slot and
    ///       `num_changed_entries` is the number of changed key-value pairs in the map.
    ///
    /// # Rationale
    ///
    /// The rationale for this layout is that hashing in the VM should be as efficient as possible
    /// and minimize the number of branches to be as efficient as possible. Every high-level section
    /// in this bullet point list should add an even number of words since the hasher operates
    /// on double words. In the VM, each permutation is done immediately, so adding an uneven
    /// number of words in a given step will result in more difficulty in the MASM implementation.
    ///
    /// # Security
    ///
    /// The general concern with the commitment is that two deltas must never has to the same
    /// commitment. E.g. a commitment of a delta that changes a key-value pair in a storage map
    /// slot should be different from a delta that adds a non-fungible asset to the vault. If
    /// not, a delta can be crafted in the VM that sets a map key but a malicious actor crafts a
    /// delta outside the VM that adds a non-fungible asset. To prevent that, a couple of
    /// measures are taken.
    ///
    /// - Because multiple unrelated contexts (e.g. vaults and storage slots) are hashed in the same
    ///   hasher, domain separators are used to disambiguate. For each changed asset and each
    ///   changed slot in the delta, a domain separator is hashed into the delta. The domain
    ///   separator is always at the same index in each layout so it cannot be maliciously crafted
    ///   (see below for an example).
    /// - Storage value slots:
    ///   - since only changed value slots are included in the delta, there is no ambiguity between
    ///     a value slot being set to EMPTY_WORD and its value being unchanged.
    /// - Storage map slots:
    ///   - Map slots append a header which summarizes the changes in the slot, in particular the
    ///     slot index and number of changed entries. Since only changed slots are included, the
    ///     number of changed entries is never zero.
    ///   - Two distinct storage map slots use the same domain but are disambiguated due to
    ///     inclusion of the slot index.
    ///
    /// **Domain Separators**
    ///
    /// As an example for ambiguity, consider these two deltas:
    ///
    /// ```text
    /// [
    ///   ID_AND_NONCE, EMPTY_WORD,
    ///   [/* no fungible asset delta */],
    ///   [[domain = 1, was_added = 1, 0, 0], NON_FUNGIBLE_ASSET],
    ///   [/* no storage delta */]
    /// ]
    /// ```
    ///
    /// ```text
    /// [
    ///   ID_AND_NONCE, EMPTY_WORD,
    ///   [/* no fungible asset delta */],
    ///   [/* no non-fungible asset delta */],
    ///   [[domain = 2, slot_idx = 1, 0, 0], NEW_VALUE]
    /// ]
    /// ```
    ///
    /// `NEW_VALUE` is user-controllable so it can be crafted to match `NON_FUNGIBLE_ASSET`. The
    /// domain separator is then the only value that differentiates these two deltas. This shows the
    /// importance of placing the domain separators in the same index within each word's layout
    /// which makes it easy to see that this value cannot be crafted to be the same.
    ///
    /// **Number of Changed Entries**
    ///
    /// As an example for ambiguity, consider these two deltas:
    ///
    /// ```text
    /// [
    ///   EMPTY_WORD, ID_AND_NONCE,
    ///   [/* no fungible asset delta */],
    ///   [[domain = 1, was_added = 1, 0, 0], NON_FUNGIBLE_ASSET],
    ///   [/* no storage delta */],
    /// ]
    /// ```
    ///
    /// ```text
    /// [
    ///    ID_AND_NONCE, EMPTY_WORD,
    ///   [/* no fungible asset delta */],
    ///   [/* no non-fungible asset delta */],
    ///   [KEY0, VALUE0],
    ///   [KEY1, VALUE1],
    ///   [domain = 3, slot_idx = 0, num_changed_entries = 2, 0, 0, 0, 0, 0]
    /// ]
    /// ```
    ///
    /// The keys and values of map slots are user-controllable so `KEY0` and `VALUE0` can be crafted
    /// to match `NON_FUNGIBLE_ASSET` and its metadata. Including the header of the map slot
    /// additionally hashes the map domain into the delta, but if the header was included whenever
    /// _any_ value in the map has changed, it would cause ambiguity about whether `KEY0`/`VALUE0`
    /// are in fact map keys or a non-fungible asset (or any asset or a value storage slot more
    /// generally). Including `num_changed_entries` disambiguates this situation, by ensuring
    /// that the delta commitment is different when, e.g. 1) a non-fungible asset and one key-value
    /// pair have changed and 2) when two key-value pairs have changed.
    pub fn to_commitment(&self) -> Word {
        <Self as SequentialCommit>::to_commitment(self)
    }
}

impl SequentialCommit for AccountDelta {
    type Commitment = Word;

    /// Reduces the delta to a sequence of field elements.
    ///
    /// See [AccountDelta::to_commitment()] for more details.
    fn to_elements(&self) -> Vec<Felt> {
        // The commitment to an empty delta is defined as the empty word.
        if self.is_empty() {
            return Vec::new();
        }

        // Minor optimization: At least 24 elements are always added.
        let mut elements = Vec::with_capacity(24);

        // ID and Nonce
        elements.extend_from_slice(&[
            self.nonce_delta,
            ZERO,
            self.account_id.suffix(),
            self.account_id.prefix().as_felt(),
        ]);
        elements.extend_from_slice(Word::empty().as_elements());

        // Vault Delta
        self.vault.append_delta_elements(&mut elements);

        // Storage Delta
        self.storage.append_delta_elements(&mut elements);

        debug_assert!(
            elements.len() % (2 * crate::WORD_SIZE) == 0,
            "expected elements to contain an even number of words, but it contained {} elements",
            elements.len()
        );

        elements
    }
}

// ACCOUNT UPDATE DETAILS
// ================================================================================================

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

// SERIALIZATION
// ================================================================================================

impl Serializable for AccountDelta {
    fn write_into<W: ByteWriter>(&self, target: &mut W) {
        self.account_id.write_into(target);
        self.storage.write_into(target);
        self.vault.write_into(target);
        self.nonce_delta.write_into(target);
    }

    fn get_size_hint(&self) -> usize {
        self.account_id.get_size_hint()
            + self.storage.get_size_hint()
            + self.vault.get_size_hint()
            + self.nonce_delta.get_size_hint()
    }
}

impl Deserializable for AccountDelta {
    fn read_from<R: ByteReader>(source: &mut R) -> Result<Self, DeserializationError> {
        let account_id = AccountId::read_from(source)?;
        let storage = AccountStorageDelta::read_from(source)?;
        let vault = AccountVaultDelta::read_from(source)?;
        let nonce_delta = Felt::read_from(source)?;

        validate_nonce(nonce_delta, &storage, &vault)
            .map_err(|err| DeserializationError::InvalidValue(err.to_string()))?;

        Ok(Self { account_id, storage, vault, nonce_delta })
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
///
/// Returns an error if:
/// - storage or vault were updated, but the nonce_delta was set to 0.
fn validate_nonce(
    nonce_delta: Felt,
    storage: &AccountStorageDelta,
    vault: &AccountVaultDelta,
) -> Result<(), AccountDeltaError> {
    if (!storage.is_empty() || !vault.is_empty()) && nonce_delta == ZERO {
        return Err(AccountDeltaError::NonEmptyStorageOrVaultDeltaWithZeroNonceDelta);
    }

    Ok(())
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {

    use assert_matches::assert_matches;
    use vm_core::{Felt, FieldElement, utils::Serializable};

    use super::{AccountDelta, AccountStorageDelta, AccountVaultDelta};
    use crate::{
        AccountDeltaError, ONE, Word, ZERO,
        account::{
            Account, AccountCode, AccountId, AccountStorage, AccountStorageMode, AccountType,
            StorageMapDelta, delta::AccountUpdateDetails,
        },
        asset::{Asset, AssetVault, FungibleAsset, NonFungibleAsset, NonFungibleAssetDetails},
        testing::account_id::{
            ACCOUNT_ID_PRIVATE_SENDER, ACCOUNT_ID_REGULAR_PRIVATE_ACCOUNT_UPDATABLE_CODE,
            AccountIdBuilder,
        },
    };

    #[test]
    fn account_delta_nonce_validation() {
        let account_id = AccountId::try_from(ACCOUNT_ID_PRIVATE_SENDER).unwrap();
        // empty delta
        let storage_delta = AccountStorageDelta::new();
        let vault_delta = AccountVaultDelta::default();

        AccountDelta::new(account_id, storage_delta.clone(), vault_delta.clone(), ZERO).unwrap();
        AccountDelta::new(account_id, storage_delta.clone(), vault_delta.clone(), ONE).unwrap();

        // non-empty delta
        let storage_delta = AccountStorageDelta::from_iters([1], [], []);

        assert_matches!(
            AccountDelta::new(account_id, storage_delta.clone(), vault_delta.clone(), ZERO)
                .unwrap_err(),
            AccountDeltaError::NonEmptyStorageOrVaultDeltaWithZeroNonceDelta
        );
        AccountDelta::new(account_id, storage_delta.clone(), vault_delta.clone(), ONE).unwrap();
    }

    #[test]
    fn account_delta_nonce_overflow() {
        let account_id = AccountId::try_from(ACCOUNT_ID_PRIVATE_SENDER).unwrap();
        let storage_delta = AccountStorageDelta::new();
        let vault_delta = AccountVaultDelta::default();

        let nonce_delta0 = ONE;
        let nonce_delta1 = Felt::try_from(0xffff_ffff_0000_0000u64).unwrap();

        let mut delta0 =
            AccountDelta::new(account_id, storage_delta.clone(), vault_delta.clone(), nonce_delta0)
                .unwrap();
        let delta1 =
            AccountDelta::new(account_id, storage_delta, vault_delta, nonce_delta1).unwrap();

        assert_matches!(delta0.merge(delta1).unwrap_err(), AccountDeltaError::NonceIncrementOverflow {
          current, increment, new
        } => {
            assert_eq!(current, nonce_delta0);
            assert_eq!(increment, nonce_delta1);
            assert_eq!(new, nonce_delta0 + nonce_delta1);
        });
    }

    #[test]
    fn account_update_details_size_hint() {
        // AccountDelta
        let account_id = AccountId::try_from(ACCOUNT_ID_PRIVATE_SENDER).unwrap();
        let storage_delta = AccountStorageDelta::new();
        let vault_delta = AccountVaultDelta::default();
        assert_eq!(storage_delta.to_bytes().len(), storage_delta.get_size_hint());
        assert_eq!(vault_delta.to_bytes().len(), vault_delta.get_size_hint());

        let account_delta =
            AccountDelta::new(account_id, storage_delta, vault_delta, ZERO).unwrap();
        assert_eq!(account_delta.to_bytes().len(), account_delta.get_size_hint());

        let storage_delta = AccountStorageDelta::from_iters(
            [1],
            [(2, Word::from([1, 1, 1, 1u32])), (3, Word::from([1, 1, 0, 1u32]))],
            [(
                4,
                StorageMapDelta::from_iters(
                    [Word::from([1, 1, 1, 0u32]), Word::from([0, 1, 1, 1u32])],
                    [(Word::from([1, 1, 1, 1u32]), Word::from([1, 1, 1, 1u32]))],
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

        let account_delta = AccountDelta::new(account_id, storage_delta, vault_delta, ONE).unwrap();
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

use alloc::collections::BTreeSet;

use miden_objects::account::{AccountId, PartialAccount, StorageMapWitness};
use miden_objects::asset::AssetWitness;
use miden_objects::block::{BlockHeader, BlockNumber};
use miden_objects::transaction::{AccountInputs, PartialBlockchain};
use miden_processor::{FutureMaybeSend, MastForestStore, Word};

use crate::DataStoreError;

// DATA STORE TRAIT
// ================================================================================================

/// The [DataStore] trait defines the interface that transaction objects use to fetch data
/// required for transaction execution.
pub trait DataStore: MastForestStore {
    /// Returns all the data required to execute a transaction against the account with the
    /// specified ID and consuming input notes created in blocks in the input `ref_blocks` set.
    ///
    /// The highest block number in `ref_blocks` will be the transaction reference block. In
    /// general, it is recommended that the reference corresponds to the latest block available
    /// in the data store.
    ///
    /// # Errors
    /// Returns an error if:
    /// - The account with the specified ID could not be found in the data store.
    /// - The block with the specified number could not be found in the data store.
    /// - The combination of specified inputs resulted in a transaction input error.
    /// - The data store encountered some internal error
    fn get_transaction_inputs(
        &self,
        account_id: AccountId,
        ref_blocks: BTreeSet<BlockNumber>,
    ) -> impl FutureMaybeSend<
        Result<(PartialAccount, Option<Word>, BlockHeader, PartialBlockchain), DataStoreError>,
    >;

    // TODO: Rename to get_foreign_account_inputs?
    /// Returns a partial foreign account state together with a proof that it is a valid account
    /// state as of the specified transaction reference block.
    fn get_partial_foreign_account(
        &self,
        foreign_account_id: AccountId,
        ref_block: BlockNumber,
    ) -> impl FutureMaybeSend<Result<AccountInputs, DataStoreError>>;

    /// Returns a witness for an asset in the requested account's vault with the requested vault
    /// root.
    ///
    /// This is the witness that needs to be added to the advice provider's merkle store and advice
    /// map to make access to the specified asset possible.
    fn get_vault_asset_witness(
        &self,
        account_id: AccountId,
        vault_root: Word,
        vault_key: Word,
    ) -> impl FutureMaybeSend<Result<AssetWitness, DataStoreError>>;

    /// Returns a witness for a storage map item identified by `map_key` in the requested account's
    /// storage with the requested storage `map_root`.
    ///
    /// Note that the `map_key` needs to be hashed in order to get the actual key into the storage
    /// map.
    ///
    /// This is the witness that needs to be added to the advice provider's merkle store and advice
    /// map to make access to the specified storage map item possible.
    fn get_storage_map_witness(
        &self,
        account_id: AccountId,
        map_root: Word,
        map_key: Word,
    ) -> impl FutureMaybeSend<Result<StorageMapWitness, DataStoreError>>;
}

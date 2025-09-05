use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::error::Error;

use miden_objects::note::NoteMetadata;
use miden_objects::transaction::TransactionSummary;
use miden_objects::{AccountDeltaError, AssetError, Felt, NoteError, Word};
use thiserror::Error;

// TRANSACTION KERNEL ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionKernelError {
    #[error("failed to add asset to account delta")]
    AccountDeltaAddAssetFailed(#[source] AccountDeltaError),
    #[error("failed to remove asset to account delta")]
    AccountDeltaRemoveAssetFailed(#[source] AccountDeltaError),
    #[error("failed to add asset to note")]
    FailedToAddAssetToNote(#[source] NoteError),
    #[error("note input data has hash {actual} but expected hash {expected}")]
    InvalidNoteInputs { expected: Word, actual: Word },
    #[error(
        "storage slot index {actual} is invalid, must be smaller than the number of account storage slots {max}"
    )]
    InvalidStorageSlotIndex { max: u64, actual: u64 },
    #[error(
        "failed to respond to signature requested since no authenticator is assigned to the host"
    )]
    MissingAuthenticator,
    #[error("failed to generate signature")]
    SignatureGenerationFailed(#[source] Box<dyn Error + Send + Sync + 'static>),
    #[error("transaction returned unauthorized event but a commitment did not match: {0}")]
    TransactionSummaryCommitmentMismatch(#[source] Box<dyn Error + Send + Sync + 'static>),
    #[error("failed to construct transaction summary")]
    TransactionSummaryConstructionFailed(#[source] Box<dyn Error + Send + Sync + 'static>),
    #[error("asset data extracted from the stack by event handler `{handler}` is not well formed")]
    MalformedAssetInEventHandler {
        handler: &'static str,
        source: AssetError,
    },
    /// A generic error returned when the transaction kernel did not behave as expected.
    #[error("tx kernel violated an assumption: {0}")]
    ViolatedAssumption(String),
    #[error(
        "note inputs data extracted from the advice map by the event handler is not well formed"
    )]
    MalformedNoteInputs(#[source] NoteError),
    #[error("note metadata created by the event handler is not well formed")]
    MalformedNoteMetadata(#[source] NoteError),
    #[error(
        "note script data `{data:?}` extracted from the advice map by the event handler is not well formed"
    )]
    MalformedNoteScript {
        data: Vec<Felt>,
        // This is always a DeserializationError, but we can't import it directly here without
        // adding dependencies, so we make it a trait object instead.
        source: Box<dyn Error + Send + Sync + 'static>,
    },
    #[error("recipient data `{0:?}` in the advice provider is not well formed")]
    MalformedRecipientData(Vec<Felt>),
    #[error("cannot add asset to note with index {0}, note does not exist in the advice provider")]
    MissingNote(u64),
    #[error(
        "public note with metadata {0:?} and recipient digest {1} is missing details in the advice provider"
    )]
    PublicNoteMissingDetails(NoteMetadata, Word),
    #[error(
        "note input data in advice provider contains fewer elements ({actual}) than specified ({specified}) by its inputs length"
    )]
    TooFewElementsForNoteInputs { specified: u64, actual: u64 },
    #[error("account procedure with procedure root {0} is not in the advice provider")]
    UnknownAccountProcedure(Word),
    #[error("code commitment {0} is not in the advice provider")]
    UnknownCodeCommitment(Word),
    #[error("account storage slots number is missing in memory at address {0}")]
    AccountStorageSlotsNumMissing(u32),
    #[error("account nonce can only be incremented once")]
    NonceCanOnlyIncrementOnce,
    #[error("failed to convert fee asset into fungible asset")]
    FailedToConvertFeeAsset(#[source] AssetError),
    #[error(
        "failed to get vault asset witness from data store for vault root {vault_root} and vault_key {vault_key}"
    )]
    GetVaultAssetWitness {
        vault_root: Word,
        vault_key: Word,
        // TODO: Change to DataStoreError when this error moves to miden-tx.
        // This is always a DataStoreError, but we can't import it from miden-tx here.
        source: Box<dyn Error + Send + Sync + 'static>,
    },
    #[error(
        "native asset amount {account_balance} in the account vault is not sufficient to cover the transaction fee of {tx_fee}"
    )]
    InsufficientFee { account_balance: u64, tx_fee: u64 },
    /// This variant signals that a signature over the contained commitments is required, but
    /// missing.
    #[error("transaction requires a signature")]
    Unauthorized(Box<TransactionSummary>),
}

// TRANSACTION EVENT PARSING ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionEventError {
    #[error("event id {0} is not a valid transaction event")]
    InvalidTransactionEvent(u32),
    #[error("event id {0} is not a transaction kernel event")]
    NotTransactionEvent(u32),
    #[error("event id {0} can only be emitted from the root context")]
    NotRootContext(u32),
}

// TRANSACTION TRACE PARSING ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionTraceParsingError {
    #[error("trace id {0} is an unknown transaction kernel trace")]
    UnknownTransactionTrace(u32),
}

#[cfg(test)]
mod error_assertions {
    use super::*;

    /// Asserts at compile time that the passed error has Send + Sync + 'static bounds.
    fn _assert_error_is_send_sync_static<E: core::error::Error + Send + Sync + 'static>(_: E) {}

    fn _assert_transaction_kernel_error_bounds(err: TransactionKernelError) {
        _assert_error_is_send_sync_static(err);
    }
}

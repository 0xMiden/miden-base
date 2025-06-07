use alloc::{boxed::Box, vec::Vec};
use core::error::Error;

use miden_objects::{AccountDeltaError, AssetError, Digest, Felt, NoteError, note::NoteMetadata};
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
    InvalidNoteInputs { expected: Digest, actual: Digest },
    #[error(
        "storage slot index {actual} is invalid, must be smaller than the number of account storage slots {max}"
    )]
    InvalidStorageSlotIndex { max: u64, actual: u64 },
    #[error("failed to push element {0} to advice stack")]
    FailedToPushAdviceStack(Felt),
    #[error("failed to generate signature: {0}")]
    FailedSignatureGeneration(&'static str),
    #[error("asset data extracted from the stack by event handler `{handler}` is not well formed")]
    MalformedAssetInEventHandler {
        handler: &'static str,
        source: AssetError,
    },
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
    PublicNoteMissingDetails(NoteMetadata, Digest),
    #[error(
        "note input data in advice provider contains fewer elements ({actual}) than specified ({specified}) by its inputs length"
    )]
    TooFewElementsForNoteInputs { specified: u64, actual: u64 },
    #[error("account procedure with procedure root {0} is not in the advice provider")]
    UnknownAccountProcedure(Digest),
    #[error("code commitment {0} is not in the advice provider")]
    UnknownCodeCommitment(Digest),
    #[error("account storage slots number is missing in memory at address {0}")]
    AccountStorageSlotsNumMissing(u32),
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

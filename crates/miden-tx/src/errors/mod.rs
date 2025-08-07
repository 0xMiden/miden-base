use alloc::boxed::Box;
use alloc::string::String;
use core::error::Error;

use miden_lib::transaction::TransactionAdviceMapMismatch;
use miden_objects::account::AccountId;
use miden_objects::assembly::diagnostics::reporting::PrintDiagnostic;
use miden_objects::block::BlockNumber;
use miden_objects::crypto::merkle::SmtProofError;
use miden_objects::note::NoteId;
use miden_objects::transaction::TransactionSummary;
use miden_objects::{
    AccountDeltaError,
    AccountError,
    Felt,
    ProvenTransactionError,
    TransactionInputError,
    TransactionOutputError,
    Word,
};
use miden_verifier::VerificationError;
use thiserror::Error;
use vm_processor::ExecutionError;

// TRANSACTION EXECUTOR ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionExecutorError {
    #[error("the advice map contains conflicting map entries")]
    ConflictingAdviceMapEntry(#[source] TransactionAdviceMapMismatch),
    #[error("failed to fetch transaction inputs from the data store")]
    FetchTransactionInputsFailed(#[source] DataStoreError),
    #[error("foreign account inputs for ID {0} are not anchored on reference block")]
    ForeignAccountNotAnchoredInReference(AccountId),
    #[error(
        "execution options' cycles must be between {min_cycles} and {max_cycles}, but found {actual}"
    )]
    InvalidExecutionOptionsCycles {
        min_cycles: u32,
        max_cycles: u32,
        actual: u32,
    },
    #[error("failed to create transaction inputs")]
    InvalidTransactionInputs(#[source] TransactionInputError),
    #[error("failed to process account update commitment: {0}")]
    AccountUpdateCommitment(&'static str),
    #[error(
        "account delta commitment computed in transaction kernel ({in_kernel_commitment}) does not match account delta computed via the host ({host_commitment})"
    )]
    InconsistentAccountDeltaCommitment {
        in_kernel_commitment: Word,
        host_commitment: Word,
    },
    #[error("failed to add or remove fee asset from transaction outputs to account delta")]
    FailedToMutateAccountDeltaWithFee(#[source] AccountDeltaError),
    #[error("input account ID {input_id} does not match output account ID {output_id}")]
    InconsistentAccountId {
        input_id: AccountId,
        output_id: AccountId,
    },
    #[error("expected account nonce delta to be {expected}, found {actual}")]
    InconsistentAccountNonceDelta { expected: Felt, actual: Felt },
    #[error("account witness provided for account ID {0} is invalid")]
    InvalidAccountWitness(AccountId, #[source] SmtProofError),
    #[error(
        "input note {0} was created in a block past the transaction reference block number ({1})"
    )]
    NoteBlockPastReferenceBlock(NoteId, BlockNumber),
    #[error("failed to create transaction host")]
    TransactionHostCreationFailed(#[source] TransactionHostError),
    #[error("failed to construct transaction outputs")]
    TransactionOutputConstructionFailed(#[source] TransactionOutputError),
    // Print the diagnostic directly instead of returning the source error. In the source error
    // case, the diagnostic is lost if the execution error is not explicitly unwrapped.
    #[error("failed to execute transaction kernel program:\n{}", PrintDiagnostic::new(.0))]
    TransactionProgramExecutionFailed(ExecutionError),
    /// This variant can be matched on to get the summary of a transaction for signing purposes.
    // It is boxed to avoid triggering clippy::result_large_err for functions that return this type.
    #[error("transaction is unauthorized with summary {0:?}")]
    Unauthorized(Box<TransactionSummary>),
}

// TRANSACTION PROVER ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionProverError {
    #[error("failed to apply account delta")]
    AccountDeltaApplyFailed(#[source] AccountError),
    #[error("failed to add or remove fee asset from transaction outputs to account delta")]
    FailedToMutateAccountDeltaWithFee(#[source] AccountDeltaError),
    #[error("failed to construct transaction outputs")]
    TransactionOutputConstructionFailed(#[source] TransactionOutputError),
    #[error("failed to build proven transaction")]
    ProvenTransactionBuildFailed(#[source] ProvenTransactionError),
    #[error("the advice map contains conflicting map entries")]
    ConflictingAdviceMapEntry(#[source] TransactionAdviceMapMismatch),
    // Print the diagnostic directly instead of returning the source error. In the source error
    // case, the diagnostic is lost if the execution error is not explicitly unwrapped.
    #[error("failed to execute transaction kernel program:\n{}", PrintDiagnostic::new(.0))]
    TransactionProgramExecutionFailed(ExecutionError),
    #[error("failed to create transaction host")]
    TransactionHostCreationFailed(#[source] TransactionHostError),
    /// Custom error variant for errors not covered by the other variants.
    #[error("{error_msg}")]
    Other {
        error_msg: Box<str>,
        // thiserror will return this when calling Error::source on DataStoreError.
        source: Option<Box<dyn Error + Send + Sync + 'static>>,
    },
}

impl TransactionProverError {
    /// Creates a custom error using the [`TransactionProverError::Other`] variant from an error
    /// message.
    pub fn other(message: impl Into<String>) -> Self {
        let message: String = message.into();
        Self::Other { error_msg: message.into(), source: None }
    }

    /// Creates a custom error using the [`TransactionProverError::Other`] variant from an error
    /// message and a source error.
    pub fn other_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        let message: String = message.into();
        Self::Other {
            error_msg: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

// TRANSACTION VERIFIER ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionVerifierError {
    #[error("failed to verify transaction")]
    TransactionVerificationFailed(#[source] VerificationError),
    #[error("transaction proof security level is {actual} but must be at least {expected_minimum}")]
    InsufficientProofSecurityLevel { actual: u32, expected_minimum: u32 },
}

// TRANSACTION HOST ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum TransactionHostError {
    #[error("{0}")]
    AccountProcedureIndexMapError(String),
    #[error("failed to create account procedure info")]
    AccountProcedureInfoCreationFailed(#[source] AccountError),
}

// DATA STORE ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum DataStoreError {
    #[error("account with id {0} not found in data store")]
    AccountNotFound(AccountId),
    #[error("block with number {0} not found in data store")]
    BlockNotFound(BlockNumber),
    /// Custom error variant for implementors of the [`DataStore`](crate::executor::DataStore)
    /// trait.
    #[error("{error_msg}")]
    Other {
        error_msg: Box<str>,
        // thiserror will return this when calling Error::source on DataStoreError.
        source: Option<Box<dyn Error + Send + Sync + 'static>>,
    },
}

impl DataStoreError {
    /// Creates a custom error using the [`DataStoreError::Other`] variant from an error message.
    pub fn other(message: impl Into<String>) -> Self {
        let message: String = message.into();
        Self::Other { error_msg: message.into(), source: None }
    }

    /// Creates a custom error using the [`DataStoreError::Other`] variant from an error message and
    /// a source error.
    pub fn other_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        let message: String = message.into();
        Self::Other {
            error_msg: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

// AUTHENTICATION ERROR
// ================================================================================================

#[derive(Debug, Error)]
pub enum AuthenticationError {
    #[error("signature rejected: {0}")]
    RejectedSignature(String),
    #[error("unknown public key: {0}")]
    UnknownPublicKey(String),
    /// Custom error variant for implementors of the
    /// [`TransactionAuthenticatior`](crate::auth::TransactionAuthenticator) trait.
    #[error("{error_msg}")]
    Other {
        error_msg: Box<str>,
        // thiserror will return this when calling Error::source on DataStoreError.
        source: Option<Box<dyn Error + Send + Sync + 'static>>,
    },
}

impl AuthenticationError {
    /// Creates a custom error using the [`AuthenticationError::Other`] variant from an error
    /// message.
    pub fn other(message: impl Into<String>) -> Self {
        let message: String = message.into();
        Self::Other { error_msg: message.into(), source: None }
    }

    /// Creates a custom error using the [`AuthenticationError::Other`] variant from an error
    /// message and a source error.
    pub fn other_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        let message: String = message.into();
        Self::Other {
            error_msg: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

#[cfg(test)]
mod error_assertions {
    use super::*;

    /// Asserts at compile time that the passed error has Send + Sync + 'static bounds.
    fn _assert_error_is_send_sync_static<E: core::error::Error + Send + Sync + 'static>(_: E) {}

    fn _assert_data_store_error_bounds(err: DataStoreError) {
        _assert_error_is_send_sync_static(err);
    }

    fn _assert_authentication_error_bounds(err: AuthenticationError) {
        _assert_error_is_send_sync_static(err);
    }
}

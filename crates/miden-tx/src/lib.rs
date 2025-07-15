#![no_std]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub use miden_objects::transaction::TransactionInputs;

mod executor;
pub use executor::{
    DataStore, ExecutionOptions, MastForestStore, NoteAccountExecution, NoteConsumptionChecker,
    NoteInputsCheck, TransactionExecutor, TransactionExecutorHost,
};

pub mod host;

mod prover;
pub use prover::{
    LocalTransactionProver, ProvingOptions, TransactionMastStore, TransactionProver,
    TransactionProverHost,
};

mod verifier;
pub use verifier::TransactionVerifier;

mod errors;
pub use errors::{
    AuthenticationError, DataStoreError, TransactionExecutorError, TransactionProverError,
    TransactionVerifierError,
};

pub mod auth;

// RE-EXPORTS
// ================================================================================================
pub use miden_objects::utils;

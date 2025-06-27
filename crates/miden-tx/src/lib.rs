#![no_std]
#![allow(clippy::uninlined_format_args)]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub use miden_objects::transaction::TransactionInputs;

mod executor;
pub use executor::{
    DataStore, MastForestStore, NoteAccountExecution, NoteConsumptionChecker, NoteInputsCheck,
    TransactionExecutor,
};

pub mod host;
pub use host::{TransactionHost, TransactionProgress};

mod prover;
pub use prover::{LocalTransactionProver, ProvingOptions, TransactionMastStore, TransactionProver};

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

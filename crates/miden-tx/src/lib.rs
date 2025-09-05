#![no_std]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

pub use miden_objects::transaction::TransactionInputs;

mod executor;
pub use executor::{
    DataStore,
    ExecutionOptions,
    FailedNote,
    MAX_NUM_CHECKER_NOTES,
    MastForestStore,
    NoteConsumptionChecker,
    NoteConsumptionInfo,
    TransactionExecutor,
    TransactionExecutorHost,
};

mod host;
pub use host::{AccountProcedureIndexMap, LinkMap, ScriptMastForestStore};

mod prover;
pub use prover::{
    LocalTransactionProver,
    ProvingOptions,
    TransactionMastStore,
    TransactionProverHost,
};

mod verifier;
pub use verifier::TransactionVerifier;

mod errors;
pub use errors::{
    AuthenticationError,
    DataStoreError,
    NoteCheckerError,
    TransactionExecutorError,
    TransactionProverError,
    TransactionVerifierError,
};

pub mod auth;

// RE-EXPORTS
// ================================================================================================
pub use miden_objects::utils;

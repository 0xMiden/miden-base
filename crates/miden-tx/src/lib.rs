#![no_std]
// TODO(named_slots): Remove.
#![allow(unused_variables)]
#![allow(dead_code)]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod executor;
pub use executor::{
    DataStore,
    ExecutionOptions,
    FailedNote,
    MAX_NUM_CHECKER_NOTES,
    MastForestStore,
    NoteConsumptionChecker,
    NoteConsumptionInfo,
    NoteConsumptionStatus,
    TransactionExecutor,
    TransactionExecutorHost,
};

mod host;
pub use host::{AccountProcedureIndexMap, LinkMap, MemoryViewer, ScriptMastForestStore};

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
    TransactionKernelError,
    TransactionProverError,
    TransactionVerifierError,
};

pub mod auth;

// RE-EXPORTS
// ================================================================================================
pub use miden_objects::utils;

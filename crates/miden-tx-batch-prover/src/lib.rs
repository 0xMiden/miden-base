#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

mod local_batch_prover;
pub use local_batch_prover::LocalBatchProver;

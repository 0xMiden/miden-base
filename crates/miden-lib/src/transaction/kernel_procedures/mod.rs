use alloc::vec::Vec;

use miden_objects::{Felt, Hasher, Word};
use procedures::KERNEL_PROCEDURES;

use super::TransactionKernel;

// Include kernel procedure roots generated in build.rs
#[rustfmt::skip]
mod procedures;

// TRANSACTION KERNEL
// ================================================================================================

impl TransactionKernel {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// Array of kernel procedures.
    pub const PROCEDURES: &'static [Word] = &KERNEL_PROCEDURES;

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns kernel procedures as vector of Felts.
    pub fn procedures_as_elements() -> Vec<Felt> {
        Word::words_as_elements(&KERNEL_PROCEDURES).to_vec()
    }

    /// Computes the accumulative hash of all kernel procedures.
    pub fn procedures_commitment() -> Word {
        Hasher::hash_elements(&Self::procedures_as_elements())
    }
}

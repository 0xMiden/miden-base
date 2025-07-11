use alloc::vec::Vec;

use kernel_v0::KERNEL0_PROCEDURES;
use miden_objects::{Felt, Hasher, Word};

use super::TransactionKernel;

// Include kernel v0 procedure roots generated in build.rs
#[rustfmt::skip]
mod kernel_v0;

// TRANSACTION KERNEL
// ================================================================================================

impl TransactionKernel {
    // CONSTANTS
    // --------------------------------------------------------------------------------------------

    /// Number of currently used kernel versions.
    pub const NUM_VERSIONS: usize = 1;

    /// Array of all available kernels.
    pub const PROCEDURES: [&'static [Word]; Self::NUM_VERSIONS] = [&KERNEL0_PROCEDURES];

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns procedures of the kernel specified by the `kernel_version` as vector of Felts.
    pub fn procedures_as_elements(kernel_version: u8) -> Vec<Felt> {
        Word::words_as_elements(
            Self::PROCEDURES
                .get(kernel_version as usize)
                .expect("provided kernel index is out of bounds"),
        )
        .to_vec()
    }

    /// Computes the accumulative hash of all procedures of the kernel specified by the
    /// `kernel_version`.
    pub fn commitment(kernel_version: u8) -> Word {
        Hasher::hash_elements(&Self::procedures_as_elements(kernel_version))
    }

    /// Computes a hash from all kernel commitments.
    pub fn kernel_commitment() -> Word {
        Hasher::hash_elements(&[Self::commitment(0).as_elements()].concat())
    }
}

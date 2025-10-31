use miden_objects::block::{ProvenBlock, SignedBlock};

// LOCAL BLOCK PROVER
// ================================================================================================

/// A local prover for blocks, proving a [`SignedBlock`] and returning a [`ProvenBlock`].
#[derive(Clone)]
pub struct LocalBlockProver {}

impl LocalBlockProver {
    /// Creates a new [`LocalBlockProver`] instance.
    pub fn new(_proof_security_level: u32) -> Self {
        // TODO: This will eventually take the security level as a parameter, but until we verify
        // batches it is ignored.
        Self {}
    }

    /// Proves the provided [`SignedBlock`] into a [`ProvenBlock`].
    ///
    /// For now this does not actually verify the batches or create a block proof, but will be added
    /// in the future.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the account witnesses provided in the signed block result in a different account tree root
    ///   than the contained previous block header commits to.
    /// - the nullifier witnesses provided in the signed block result in a different nullifier tree
    ///   root than the contained previous block header commits to.
    /// - the account tree root in the previous block header does not match the root of the tree
    ///   computed from the account witnesses.
    /// - the nullifier tree root in the previous block header does not match the root of the tree
    ///   computed from the nullifier witnesses.
    pub fn prove(&self, signed_block: SignedBlock) -> ProvenBlock {
        self.prove_without_batch_verification_inner(signed_block)
    }

    /// Proves the provided [`SignedBlock`] into a [`ProvenBlock`], **without verifying batches
    /// and proving the block**.
    ///
    /// This is exposed for testing purposes.
    #[cfg(any(feature = "testing", test))]
    pub fn prove_dummy(&self, signed_block: SignedBlock) -> ProvenBlock {
        self.prove_without_batch_verification_inner(signed_block)
    }

    /// Proves the provided [`SignedBlock`] into a [`ProvenBlock`].
    ///
    /// See [`Self::prove`] for more details.
    fn prove_without_batch_verification_inner(&self, signed_block: SignedBlock) -> ProvenBlock {
        // Deconstruct signed block into its components.
        let (header, body) = signed_block.into_parts();

        // For now, we're not actually proving the block. Just return the block.
        ProvenBlock::new_unchecked(header, body)
    }
}

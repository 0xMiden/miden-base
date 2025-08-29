use alloc::boxed::Box;

use miden_objects::ProvenBatchError;
use miden_objects::batch::{ProposedBatch, ProvenBatch};
use miden_tx::TransactionVerifier;

// LOCAL BATCH PROVER
// ================================================================================================

/// A local prover for transaction batches, proving the transactions in a [`ProposedBatch`] and
/// returning a [`ProvenBatch`].
#[derive(Clone)]
pub struct LocalBatchProver {
    proof_security_level: u32,
}

impl LocalBatchProver {
    /// Creates a new [`LocalBatchProver`] instance.
    pub fn new(proof_security_level: u32) -> Self {
        Self { proof_security_level }
    }

    /// Attempts to prove the [`ProposedBatch`] into a [`ProvenBatch`].
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - a proof of any transaction in the batch fails to verify.
    fn prove_inner(
        &self,
        proposed_batch: ProposedBatch,
        verify_txns: bool,
    ) -> Result<ProvenBatch, ProvenBatchError> {
        let tx_headers = proposed_batch.transaction_headers();
        let (
            transactions,
            block_header,
            _block_chain,
            _authenticatable_unauthenticated_notes,
            id,
            updated_accounts,
            input_notes,
            output_notes,
            batch_expiration_block_num,
        ) = proposed_batch.into_parts();

        // Only verify transactions if `verify_txns` is true.
        // For testing purposes, we don't want to verify transactions.
        if verify_txns {
            let verifier = TransactionVerifier::new(self.proof_security_level);

            for tx in transactions {
                verifier.verify(&tx).map_err(|source| {
                    ProvenBatchError::TransactionVerificationFailed {
                        transaction_id: tx.id(),
                        source: Box::new(source),
                    }
                })?;
            }
        }

        ProvenBatch::new(
            id,
            block_header.commitment(),
            block_header.block_num(),
            updated_accounts,
            input_notes,
            output_notes,
            batch_expiration_block_num,
            tx_headers,
        )
    }

    pub fn prove(&self, proposed_batch: ProposedBatch) -> Result<ProvenBatch, ProvenBatchError> {
        self.prove_inner(proposed_batch, true)
    }

    /// Proves the provided [`ProposedBatch`] into a [`ProvenBatch`], **without verifying batches
    /// and proving the block**.
    ///
    /// This is exposed for testing purposes.
    #[cfg(any(feature = "testing", test))]
    pub fn dummy_prove(
        &self,
        proposed_batch: ProposedBatch,
    ) -> Result<ProvenBatch, ProvenBatchError> {
        self.prove_inner(proposed_batch, false)
    }
}

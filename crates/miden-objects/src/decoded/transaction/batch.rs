pub use proto::transaction::DecodedBatchAccountUpdate as BatchAccountUpdate;

use super::{OutputNoteError, ProvenTransactionError, TransactionHeaderBuildError};
use crate::{BuildUnchecked, Verify, proto};

#[cfg(test)]
mod tests;

impl Verify for BatchAccountUpdate {
    type Verified = miden_protocol::batch::BatchAccountUpdate;
    type Error = BatchAccountUpdateError;
    fn verify(self) -> Result<Self::Verified, Self::Error> {
        Ok(Self::Verified::new(
            self.account_id.verify()?,
            self.initial_state_commitment,
            self.final_state_commitment,
            self.details.verify()?,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BatchAccountUpdateError {
    #[error("{0}")]
    AccountId(#[from] miden_protocol::errors::AccountIdError),
    #[error("{0}")]
    Details(#[from] crate::decoded::account::AccountPatchError),
    #[error("{0}")]
    Update(#[from] miden_protocol::errors::BatchAccountUpdateError),
}

pub use proto::transaction::DecodedProposedBatch as ProposedBatch;

/// Verifies transaction proofs and batch consistency, not trust in the supplied reference chain.
impl crate::VerifyWith<u32> for ProposedBatch {
    type Verified = miden_protocol::batch::ProposedBatch;
    type Error = ProposedBatchError;
    fn verify_with(self, proof_security_level: u32) -> Result<Self::Verified, Self::Error> {
        let transactions = self
            .transactions
            .into_iter()
            .map(|tx| tx.build_unchecked().map(alloc::sync::Arc::new))
            .collect::<Result<_, _>>()?;
        let header = self.reference_block_header.build_unchecked()?;
        let chain = self.partial_blockchain.build_unchecked()?;
        let mut proofs = alloc::collections::BTreeMap::new();
        let mut previous = None;
        for proof in self.unauthenticated_note_proofs {
            let (id, proof) = proof.verify()?;
            if previous.is_some_and(|previous| id <= previous) {
                return Err(ProposedBatchError::ProofOrder);
            }
            previous = Some(id);
            proofs.insert(id, proof);
        }
        Ok(Self::Verified::new(transactions, header, chain, proofs, proof_security_level)?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProposedBatchError {
    #[error("{0}")]
    Transaction(#[from] ProvenTransactionError),
    #[error("{0}")]
    Header(#[from] crate::decoded::blockchain::BlockHeaderError),
    #[error("{0}")]
    Chain(#[from] crate::decoded::blockchain::PartialBlockchainError),
    #[error("{0}")]
    Note(#[from] crate::decoded::note::VerificationError),
    #[error("unauthenticated note proofs must have unique, ascending note IDs")]
    ProofOrder,
    #[error("{0}")]
    Batch(#[from] miden_protocol::errors::ProposedBatchError),
}

pub use proto::transaction::DecodedProvenBatch as ProvenBatch;

/// Checks local batch invariants, but not proof validity, note aggregation, or transaction
/// ordering.
impl crate::BuildUnchecked for ProvenBatch {
    type Output = miden_protocol::batch::ProvenBatch;
    type Error = ProvenBatchError;
    fn build_unchecked(self) -> Result<Self::Output, Self::Error> {
        let mut previous = None;
        let mut updates = alloc::vec::Vec::new();
        for update in self.account_updates {
            let update = update.verify()?;
            if previous.is_some_and(|previous| update.account_id() <= previous) {
                return Err(ProvenBatchError::AccountOrder);
            }
            previous = Some(update.account_id());
            updates.push(update);
        }
        let inputs = self
            .input_notes
            .into_iter()
            .map(BuildUnchecked::build_unchecked)
            .collect::<Result<_, _>>()?;
        let outputs =
            self.output_notes.into_iter().map(Verify::verify).collect::<Result<_, _>>()?;
        let transactions = self
            .transactions
            .into_iter()
            .map(BuildUnchecked::build_unchecked)
            .collect::<Result<_, _>>()?;
        Ok(Self::Output::new(
            self.reference_block_commitment,
            self.reference_block_num.verify().expect("infallible block number"),
            updates,
            miden_protocol::transaction::InputNotes::new_unchecked(inputs),
            outputs,
            self.expiration_block_num.verify().expect("infallible block number"),
            miden_protocol::transaction::OrderedTransactionHeaders::new_unchecked(transactions),
            self.proof,
        )?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProvenBatchError {
    #[error("{0}")]
    Update(#[from] BatchAccountUpdateError),
    #[error("{0}")]
    Input(#[from] crate::decoded::note::VerificationError),
    #[error("{0}")]
    Output(#[from] OutputNoteError),
    #[error("{0}")]
    Transaction(#[from] TransactionHeaderBuildError),
    #[error("{0}")]
    Batch(#[from] miden_protocol::errors::ProvenBatchError),
    #[error("account updates must have unique, ascending account IDs")]
    AccountOrder,
    #[error("{0} does not match proposal")]
    ProposalMismatch(&'static str),
}

/// Checks all fields duplicated from an already-verified proposal. The batch execution proof
/// still needs verification by the consuming service; this only establishes proposal agreement.
impl crate::VerifyWith<&miden_protocol::batch::ProposedBatch> for ProvenBatch {
    type Verified = miden_protocol::batch::ProvenBatch;
    type Error = ProvenBatchError;
    fn verify_with(
        self,
        proposed: &miden_protocol::batch::ProposedBatch,
    ) -> Result<Self::Verified, Self::Error> {
        let batch = self.build_unchecked()?;
        let header = proposed.reference_block_header();
        let mismatch = if batch.reference_block_num() != header.block_num() {
            Some("reference block number")
        } else if batch.reference_block_commitment() != header.commitment() {
            Some("reference block commitment")
        } else if batch.account_updates() != proposed.account_updates() {
            Some("account updates")
        } else if !batch.input_notes().iter().eq(proposed.input_notes().iter()) {
            Some("input notes")
        } else if batch.output_notes() != proposed.output_notes() {
            Some("output notes")
        } else if batch.batch_expiration_block_num() != proposed.batch_expiration_block_num() {
            Some("expiration block")
        } else if batch.transactions().as_slice() != proposed.transaction_headers().as_slice() {
            Some("transaction headers")
        } else {
            None
        };
        if let Some(mismatch) = mismatch {
            return Err(ProvenBatchError::ProposalMismatch(mismatch));
        }
        Ok(batch)
    }
}

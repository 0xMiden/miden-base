use miden_protocol::batch::{BatchAccountUpdate, ProposedBatch, ProvenBatch};

use crate::{ConversionError, proto};

impl From<&BatchAccountUpdate> for proto::transaction::BatchAccountUpdate {
    fn from(value: &BatchAccountUpdate) -> Self {
        Self {
            account_id: Some(value.account_id().into()),
            initial_state_commitment: Some(value.initial_state_commitment().into()),
            final_state_commitment: Some(value.final_state_commitment().into()),
            details: Some(value.details().into()),
        }
    }
}

impl From<&ProposedBatch> for proto::transaction::ProposedBatch {
    fn from(value: &ProposedBatch) -> Self {
        let (transactions, reference_block_header, partial_blockchain, note_proofs, ..) =
            value.clone().into_parts();
        Self {
            transactions: transactions.iter().map(|tx| tx.as_ref().into()).collect(),
            reference_block_header: Some(reference_block_header.into()),
            partial_blockchain: Some((&partial_blockchain).into()),
            unauthenticated_note_proofs: note_proofs.iter().map(Into::into).collect(),
        }
    }
}

impl From<ProposedBatch> for proto::transaction::ProposedBatch {
    fn from(value: ProposedBatch) -> Self {
        Self::from(&value)
    }
}

/// Decodes and structurally validates a proposed batch, including transaction proof verification.
///
/// Callers handling untrusted requests should invoke this in a blocking task.

pub fn decode_proposed_batch(
    value: proto::transaction::ProposedBatch,
    proof_security_level: u32,
) -> Result<ProposedBatch, ConversionError> {
    use crate::{DecodeMessage, VerifyWith};
    value
        .decode_fields()?
        .verify_with(proof_security_level)
        .map_err(ConversionError::new)
}

impl From<&ProvenBatch> for proto::transaction::ProvenBatch {
    fn from(value: &ProvenBatch) -> Self {
        Self {
            reference_block_commitment: Some(value.reference_block_commitment().into()),
            reference_block_num: Some(value.reference_block_num().into()),
            account_updates: value.account_updates().values().map(Into::into).collect(),
            input_notes: value.input_notes().iter().map(Into::into).collect(),
            output_notes: value.output_notes().iter().map(Into::into).collect(),
            expiration_block_num: Some(value.batch_expiration_block_num().into()),
            transactions: value.transactions().as_slice().iter().map(Into::into).collect(),
            proof: Some(value.proof().into()),
        }
    }
}

impl From<ProvenBatch> for proto::transaction::ProvenBatch {
    fn from(value: ProvenBatch) -> Self {
        Self::from(&value)
    }
}

/// Decodes a proven batch without a proposal and validates every invariant available from the
/// transmitted fields. Cryptographic proof verification remains a service-boundary concern.
pub fn decode_standalone_proven_batch(
    value: proto::transaction::ProvenBatch,
) -> Result<ProvenBatch, ConversionError> {
    crate::BuildUnchecked::build_unchecked(crate::DecodeMessage::decode_fields(value)?)
        .map_err(ConversionError::new)
}

/// Decodes a proven batch and checks every public field duplicated from its proposal.

pub fn decode_proven_batch(
    value: proto::transaction::ProvenBatch,
    proposed: &ProposedBatch,
) -> Result<ProvenBatch, ConversionError> {
    use crate::{DecodeMessage, VerifyWith};
    value.decode_fields()?.verify_with(proposed).map_err(ConversionError::new)
}

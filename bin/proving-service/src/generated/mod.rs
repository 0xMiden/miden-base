use miden_objects::{
    batch::ProposedBatch,
    block::ProposedBlock,
    transaction::{ProvenTransaction, TransactionWitness},
};
use miden_tx::utils::{Deserializable, DeserializationError, Serializable};

use crate::api::prover::ProverType;

#[rustfmt::skip]
pub mod proving_service;
#[rustfmt::skip]
pub mod status;

pub use proving_service::*;

// CONVERSIONS
// ================================================================================================

impl From<ProvenTransaction> for ProvingResponse {
    fn from(value: ProvenTransaction) -> Self {
        ProvingResponse { payload: value.to_bytes() }
    }
}

impl TryFrom<ProvingResponse> for ProvenTransaction {
    type Error = DeserializationError;

    fn try_from(response: ProvingResponse) -> Result<Self, Self::Error> {
        ProvenTransaction::read_from_bytes(&response.payload)
    }
}

impl TryFrom<ProvingRequest> for TransactionWitness {
    type Error = DeserializationError;

    fn try_from(request: ProvingRequest) -> Result<Self, Self::Error> {
        TransactionWitness::read_from_bytes(&request.payload)
    }
}

impl TryFrom<ProvingRequest> for ProposedBatch {
    type Error = DeserializationError;

    fn try_from(request: ProvingRequest) -> Result<Self, Self::Error> {
        ProposedBatch::read_from_bytes(&request.payload)
    }
}

impl TryFrom<ProvingRequest> for ProposedBlock {
    type Error = DeserializationError;

    fn try_from(request: ProvingRequest) -> Result<Self, Self::Error> {
        ProposedBlock::read_from_bytes(&request.payload)
    }
}

impl From<ProverType> for ProofType {
    fn from(value: ProverType) -> Self {
        match value {
            ProverType::Transaction => ProofType::Transaction,
            ProverType::Batch => ProofType::Batch,
            ProverType::Block => ProofType::Block,
        }
    }
}

impl From<ProofType> for ProverType {
    fn from(value: ProofType) -> Self {
        match value {
            ProofType::Transaction => ProverType::Transaction,
            ProofType::Batch => ProverType::Batch,
            ProofType::Block => ProverType::Block,
        }
    }
}

impl TryFrom<i32> for ProverType {
    type Error = String;
    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ProverType::Transaction),
            1 => Ok(ProverType::Batch),
            2 => Ok(ProverType::Block),
            _ => Err(format!("unknown ProverType value: {}", value)),
        }
    }
}

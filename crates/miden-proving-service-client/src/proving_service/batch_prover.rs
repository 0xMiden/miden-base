use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use miden_objects::{
    batch::{ProposedBatch, ProvenBatch},
    transaction::{OutputNote, ProvenTransaction, TransactionHeader, TransactionId},
    utils::{Deserializable, DeserializationError, Serializable},
};
use tokio::sync::Mutex;

use super::generated::api_client::ApiClient;
use crate::{
    RemoteProverError,
    proving_service::generated::{ProofType, ProvingRequest, ProvingResponse},
};

// REMOTE BATCH PROVER
// ================================================================================================

/// A [`RemoteBatchProver`] is a batch prover that sends a proposed batch data to a remote
/// gRPC server and receives a proven batch.
///
/// When compiled for the `wasm32-unknown-unknown` target, it uses the `tonic_web_wasm_client`
/// transport. Otherwise, it uses the built-in `tonic::transport` for native platforms.
///
/// The transport layer connection is established lazily when the first batch is proven.
#[derive(Clone)]
pub struct RemoteBatchProver {
    #[cfg(target_arch = "wasm32")]
    client: Arc<Mutex<Option<ApiClient<tonic_web_wasm_client::Client>>>>,

    #[cfg(not(target_arch = "wasm32"))]
    client: Arc<Mutex<Option<ApiClient<tonic::transport::Channel>>>>,

    endpoint: String,
}

impl RemoteBatchProver {
    /// Creates a new [RemoteBatchProver] with the specified gRPC server endpoint. The
    /// endpoint should be in the format `{protocol}://{hostname}:{port}`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        RemoteBatchProver {
            endpoint: endpoint.into(),
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// Establishes a connection to the remote batch prover server. The connection is
    /// maintained for the lifetime of the prover. If the connection is already established, this
    /// method does nothing.
    async fn connect(&self) -> Result<(), RemoteProverError> {
        let mut client = self.client.lock().await;
        if client.is_some() {
            return Ok(());
        }

        #[cfg(target_arch = "wasm32")]
        let new_client = {
            let web_client = tonic_web_wasm_client::Client::new(self.endpoint.clone());
            ApiClient::new(web_client)
        };

        #[cfg(not(target_arch = "wasm32"))]
        let new_client = {
            ApiClient::connect(self.endpoint.clone())
                .await
                .map_err(|_| RemoteProverError::ConnectionFailed(self.endpoint.to_string()))?
        };

        *client = Some(new_client);

        Ok(())
    }
}

impl RemoteBatchProver {
    pub async fn prove(
        &self,
        proposed_batch: ProposedBatch,
    ) -> Result<ProvenBatch, RemoteProverError> {
        use miden_objects::utils::Serializable;
        self.connect().await?;

        let mut client = self
            .client
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| {
                RemoteProverError::ConnectionFailed("client should be connected".into())
            })?
            .clone();

        // Create the set of the transactions we pass to the prover for later validation.
        let proposed_txs: Vec<_> = proposed_batch.transactions().iter().map(Arc::clone).collect();

        let request = tonic::Request::new(proposed_batch.into());

        let response = client
            .prove(request)
            .await
            .map_err(|err| RemoteProverError::other_with_source("failed to prove block", err))?;

        // Deserialize the response bytes back into a ProvenBatch.
        let proven_batch = ProvenBatch::try_from(response.into_inner()).map_err(|err| {
            RemoteProverError::other_with_source(
                "failed to deserialize received response from remote prover",
                err,
            )
        })?;

        Self::validate_tx_headers(&proven_batch, proposed_txs)?;

        Ok(proven_batch)
    }

    /// Validates that the proven batch's transaction headers are consistent with the transactions
    /// passed in the proposed batch.
    ///
    /// Note that we expect all input and output notes from a proposed transaction to be present
    /// in the corresponding header as well, because note erasure doesn't matter for the transaction
    /// itself and we want the original transaction data to be preserved.
    ///
    /// This expects that proposed transactions and batch transactions are in the same order, as
    /// define by `OrderedTransactionHeaders`.
    fn validate_tx_headers(
        proven_batch: &ProvenBatch,
        proposed_txs: Vec<Arc<ProvenTransaction>>,
    ) -> Result<(), RemoteProverError> {
        if proposed_txs.len() != proven_batch.transactions().as_slice().len() {
            return Err(RemoteProverError::other(format!(
                "remote prover returned {} transaction headers but {} transactions were passed as part of the proposed batch",
                proven_batch.transactions().as_slice().len(),
                proposed_txs.len()
            )));
        }

        // Because we checked the length matches we can zip the iterators up.
        // We expect the transactions to be in the same order.
        for (proposed_header, proven_header) in
            proposed_txs.into_iter().zip(proven_batch.transactions().as_slice())
        {
            if proven_header.account_id() != proposed_header.account_id() {
                return Err(RemoteProverError::other(format!(
                    "transaction header of {} has a different account ID than the proposed transaction",
                    proposed_header.id()
                )));
            }

            if proven_header.initial_state_commitment()
                != proposed_header.account_update().initial_state_commitment()
            {
                return Err(RemoteProverError::other(format!(
                    "transaction header of {} has a different initial state commitment than the proposed transaction",
                    proposed_header.id()
                )));
            }

            if proven_header.final_state_commitment()
                != proposed_header.account_update().final_state_commitment()
            {
                return Err(RemoteProverError::other(format!(
                    "transaction header of {} has a different final state commitment than the proposed transaction",
                    proposed_header.id()
                )));
            }

            // Check input notes
            if (proposed_header.input_notes().num_notes() as usize)
                != proven_header.input_notes().len()
            {
                return Err(RemoteProverError::other(format!(
                    "transaction header of {} has a different number of input notes than the proposed transaction",
                    proposed_header.id()
                )));
            }

            // Because we checked the length matches we can zip the iterators up.
            // We expect the nullifiers to be in the same order.
            for (proposed_nullifier, header_nullifier) in
                proposed_header.nullifiers().zip(proven_header.input_notes().iter())
            {
                if proposed_nullifier != *header_nullifier {
                    return Err(RemoteProverError::other(format!(
                        "transaction header of {} has a different set of input notes than the proposed transaction",
                        proposed_header.id()
                    )));
                }
            }

            // Check output notes
            if proposed_header.output_notes().num_notes() != proven_header.output_notes().len() {
                return Err(RemoteProverError::other(format!(
                    "transaction header of {} has a different number of output notes than the proposed transaction",
                    proposed_header.id()
                )));
            }

            // Because we checked the length matches we can zip the iterators up.
            // We expect the note IDs to be in the same order.
            for (proposed_note_id, header_note_id) in proposed_header
                .output_notes()
                .iter()
                .map(OutputNote::id)
                .zip(proven_header.output_notes().iter())
            {
                if proposed_note_id != *header_note_id {
                    return Err(RemoteProverError::other(format!(
                        "transaction header of {} has a different set of input notes than the proposed transaction",
                        proposed_header.id()
                    )));
                }
            }
        }

        Ok(())
    }
}

// CONVERSIONS
// ================================================================================================

impl From<ProposedBatch> for ProvingRequest {
    fn from(proposed_batch: ProposedBatch) -> Self {
        ProvingRequest {
            proof_type: ProofType::Batch.into(),
            payload: proposed_batch.to_bytes(),
        }
    }
}

impl TryFrom<ProvingResponse> for ProvenBatch {
    type Error = DeserializationError;

    fn try_from(response: ProvingResponse) -> Result<Self, Self::Error> {
        ProvenBatch::read_from_bytes(&response.payload)
    }
}

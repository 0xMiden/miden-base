use alloc::{
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};

use miden_objects::{
    batch::ProvenBatch,
    block::{ProposedBlock, ProvenBlock},
    transaction::{OrderedTransactionHeaders, TransactionHeader},
    utils::{Deserializable, DeserializationError, Serializable},
};
use miden_tx::{TransactionProver, TransactionProverError};
use tokio::sync::Mutex;

use super::generated::api_client::ApiClient;
use crate::{
    RemoteProverError,
    proving_service::generated::{ProofType, ProvingRequest, ProvingResponse},
};

// REMOTE BLOCK PROVER
// ================================================================================================

/// A [`RemoteBlockProver`] is a block prover that sends a proposed block data to a remote
/// gRPC server and receives a proven block.
///
/// When compiled for the `wasm32-unknown-unknown` target, it uses the `tonic_web_wasm_client`
/// transport. Otherwise, it uses the built-in `tonic::transport` for native platforms.
///
/// The transport layer connection is established lazily when the first transaction is proven.
#[derive(Clone)]
pub struct RemoteBlockProver {
    #[cfg(target_arch = "wasm32")]
    client: Arc<Mutex<Option<ApiClient<tonic_web_wasm_client::Client>>>>,

    #[cfg(not(target_arch = "wasm32"))]
    client: Arc<Mutex<Option<ApiClient<tonic::transport::Channel>>>>,

    endpoint: String,
}

impl RemoteBlockProver {
    /// Creates a new [RemoteBlockProver] with the specified gRPC server endpoint. The
    /// endpoint should be in the format `{protocol}://{hostname}:{port}`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        RemoteBlockProver {
            endpoint: endpoint.into(),
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// Establishes a connection to the remote block prover server. The connection is
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
                .map_err(|err| RemoteProverError::ConnectionFailed(err.into()))?
        };

        *client = Some(new_client);

        Ok(())
    }
}

impl RemoteBlockProver {
    pub async fn prove(
        &self,
        proposed_block: ProposedBlock,
    ) -> Result<ProvenBlock, RemoteProverError> {
        use miden_objects::utils::Serializable;
        self.connect().await?;

        let mut client = self
            .client
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| RemoteProverError::other("client should be connected"))?
            .clone();

        // Get the set of expected transaction headers.
        let proposed_txs = proposed_block.batches().to_transactions();

        let request = tonic::Request::new(proposed_block.into());

        let response = client
            .prove(request)
            .await
            .map_err(|err| RemoteProverError::other_with_source("failed to prove block", err))?;

        // Deserialize the response bytes back into a ProvenBlock.
        let proven_block = ProvenBlock::try_from(response.into_inner()).map_err(|err| {
            RemoteProverError::other_with_source(
                "failed to deserialize received response from remote block prover",
                err,
            )
        })?;

        Self::validate_tx_headers(&proven_block, proposed_txs)?;

        Ok(proven_block)
    }

    /// Validates that the proven block's transaction headers are consistent with the transactions
    /// passed in the proposed block.
    ///
    /// This expects that transactions from the proposed block and proven block are in the same
    /// order, as define by [`OrderedTransactionHeaders`].
    fn validate_tx_headers(
        proven_block: &ProvenBlock,
        proposed_txs: OrderedTransactionHeaders,
    ) -> Result<(), RemoteProverError> {
        if proposed_txs.as_slice().len() != proven_block.transactions().as_slice().len() {
            return Err(RemoteProverError::other(format!(
                "remote prover returned {} transaction headers but {} transactions were passed as part of the proposed block",
                proven_block.transactions().as_slice().len(),
                proposed_txs.as_slice().len()
            )));
        }

        // Because we checked the length matches we can zip the iterators up.
        // We expect the transaction headers to be in the same order.
        for (proposed_header, proven_header) in
            proposed_txs.as_slice().iter().zip(proven_block.transactions().as_slice())
        {
            if proposed_header != proven_header {
                return Err(RemoteProverError::other(format!(
                    "transaction header with id {} does not match header of the transaction in the proposed block",
                    proposed_header.id()
                )));
            }
        }

        Ok(())
    }
}

// CONVERSION
// ================================================================================================

impl TryFrom<ProvingResponse> for ProvenBlock {
    type Error = DeserializationError;

    fn try_from(value: ProvingResponse) -> Result<Self, Self::Error> {
        ProvenBlock::read_from_bytes(&value.payload)
    }
}

impl From<ProposedBlock> for ProvingRequest {
    fn from(proposed_block: ProposedBlock) -> Self {
        ProvingRequest {
            proof_type: ProofType::Block.into(),
            payload: proposed_block.to_bytes(),
        }
    }
}

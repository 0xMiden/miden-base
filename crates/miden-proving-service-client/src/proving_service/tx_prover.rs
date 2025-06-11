use alloc::{
    boxed::Box,
    string::{String, ToString},
    sync::Arc,
};
use core::time::Duration;

use miden_objects::{
    transaction::{ProvenTransaction, TransactionWitness},
    utils::{Deserializable, DeserializationError, Serializable},
};
use miden_tx::{TransactionProver, TransactionProverError};
use tokio::sync::Mutex;

use super::generated::api_client::ApiClient;
use crate::{
    RemoteProverError,
    proving_service::{
        generated,
        generated::{ProofType, ProvingRequest, ProvingResponse},
    },
};

// REMOTE TRANSACTION PROVER
// ================================================================================================

/// A [RemoteTransactionProver] is a transaction prover that sends witness data to a remote
/// gRPC server and receives a proven transaction.
///
/// When compiled for the `wasm32-unknown-unknown` target, it uses the `tonic_web_wasm_client`
/// transport. Otherwise, it uses the built-in `tonic::transport` for native platforms.
///
/// The transport layer connection is established lazily when the first transaction is proven.
pub struct RemoteTransactionProver {
    #[cfg(target_arch = "wasm32")]
    client: Arc<Mutex<Option<ApiClient<tonic_web_wasm_client::Client>>>>,

    #[cfg(not(target_arch = "wasm32"))]
    client: Arc<Mutex<Option<ApiClient<tonic::transport::Channel>>>>,

    endpoint: String,
}

impl RemoteTransactionProver {
    /// Creates a new [RemoteTransactionProver] with the specified gRPC server endpoint. The
    /// endpoint should be in the format `{protocol}://{hostname}:{port}`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        RemoteTransactionProver {
            endpoint: endpoint.into(),
            client: Arc::new(Mutex::new(None)),
        }
    }

    /// Establishes a connection to the remote transaction prover server. The connection is
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
            let endpoint = tonic::transport::Endpoint::try_from(self.endpoint.clone())
                .map_err(|err| RemoteProverError::ConnectionFailed(err.into()))?
                .timeout(Duration::from_millis(10000));
            let channel = endpoint
                .tls_config(tonic::transport::ClientTlsConfig::new().with_native_roots())
                .map_err(|err| RemoteProverError::ConnectionFailed(err.into()))?
                .connect()
                .await
                .map_err(|err| RemoteProverError::ConnectionFailed(err.into()))?;

            ApiClient::connect(self.endpoint.clone())
                .await
                .map_err(|err| RemoteProverError::ConnectionFailed(err.into()))?
        };

        *client = Some(new_client);

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl TransactionProver for RemoteTransactionProver {
    async fn prove(
        &self,
        tx_witness: TransactionWitness,
    ) -> Result<ProvenTransaction, TransactionProverError> {
        use miden_objects::utils::Serializable;
        self.connect().await.map_err(|err| {
            TransactionProverError::other_with_source("failed to connect to the remote prover", err)
        })?;

        let mut client = self
            .client
            .lock()
            .await
            .as_ref()
            .ok_or_else(|| TransactionProverError::other("client should be connected"))?
            .clone();

        let request = tonic::Request::new(tx_witness.into());

        let response = client.prove(request).await.map_err(|err| {
            TransactionProverError::other_with_source("failed to prove transaction", err)
        })?;

        // Deserialize the response bytes back into a ProvenTransaction.
        let proven_transaction =
            ProvenTransaction::try_from(response.into_inner()).map_err(|_| {
                TransactionProverError::other(
                    "failed to deserialize received response from remote transaction prover",
                )
            })?;

        Ok(proven_transaction)
    }
}

// CONVERSIONS
// ================================================================================================

impl TryFrom<ProvingResponse> for ProvenTransaction {
    type Error = DeserializationError;

    fn try_from(response: ProvingResponse) -> Result<Self, Self::Error> {
        ProvenTransaction::read_from_bytes(&response.payload)
    }
}

impl From<TransactionWitness> for ProvingRequest {
    fn from(witness: TransactionWitness) -> Self {
        ProvingRequest {
            proof_type: ProofType::Transaction.into(),
            payload: witness.to_bytes(),
        }
    }
}

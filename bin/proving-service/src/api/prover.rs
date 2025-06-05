use miden_block_prover::LocalBlockProver;
use miden_objects::{
    MIN_PROOF_SECURITY_LEVEL, batch::ProposedBatch, block::ProposedBlock,
    transaction::TransactionWitness, utils::Serializable,
};
use miden_tx::{LocalTransactionProver, TransactionProver};
use miden_tx_batch_prover::LocalBatchProver;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use tracing::instrument;

use crate::generated::{ProofType, ProvingRequest, ProvingResponse, api_server::Api as ProverApi};

pub const MIDEN_PROVING_SERVICE: &str = "miden-proving-service";

/// Specifies the type of proving supported.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum ProverType {
    /// Transaction proving
    #[default]
    Transaction,
    /// Batch proving
    Batch,
    /// Block proving
    Block,
}

impl ProverType {
    /// Returns the corresponding ProofType from the generated code
    pub fn to_proof_type(&self) -> crate::generated::ProofType {
        match self {
            ProverType::Transaction => crate::generated::ProofType::Transaction,
            ProverType::Batch => crate::generated::ProofType::Batch,
            ProverType::Block => crate::generated::ProofType::Block,
        }
    }
}

impl std::fmt::Display for ProverType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProverType::Transaction => write!(f, "transaction"),
            ProverType::Batch => write!(f, "batch"),
            ProverType::Block => write!(f, "block"),
        }
    }
}

impl std::str::FromStr for ProverType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "transaction" => Ok(ProverType::Transaction),
            "batch" => Ok(ProverType::Batch),
            "block" => Ok(ProverType::Block),
            _ => Err(format!("Invalid proof type: {}", s)),
        }
    }
}

/// The prover for the proving service.
///
/// This enum is used to store the prover for the proving service.
/// Only one prover is enabled at a time.
enum Prover {
    Transaction(Mutex<LocalTransactionProver>),
    Batch(Mutex<LocalBatchProver>),
    Block(Mutex<LocalBlockProver>),
}

impl Prover {
    fn new(prover_type: ProverType) -> Self {
        match prover_type {
            ProverType::Transaction => {
                Self::Transaction(Mutex::new(LocalTransactionProver::default()))
            },
            ProverType::Batch => {
                Self::Batch(Mutex::new(LocalBatchProver::new(MIN_PROOF_SECURITY_LEVEL)))
            },
            ProverType::Block => {
                Self::Block(Mutex::new(LocalBlockProver::new(MIN_PROOF_SECURITY_LEVEL)))
            },
        }
    }
}

pub struct ProverRpcApi {
    prover: Prover,
}

impl ProverRpcApi {
    pub fn new(prover_type: ProverType) -> Self {
        let prover = Prover::new(prover_type);

        Self { prover }
    }

    #[instrument(
        target = MIDEN_PROVING_SERVICE,
        name = "proving_service:prove_tx",
        skip_all,
        ret(level = "debug"),
        fields(id = tracing::field::Empty),
        err
    )]
    pub fn prove_tx(
        &self,
        transaction_witness: TransactionWitness,
    ) -> Result<Response<ProvingResponse>, tonic::Status> {
        let prover = match &self.prover {
            Prover::Transaction(prover) => prover,
            _ => return Err(Status::unimplemented("Transaction prover is not enabled")),
        };

        let proof = prover
            .try_lock()
            .map_err(|_| Status::resource_exhausted("Server is busy handling another request"))?
            .prove(transaction_witness)
            .map_err(internal_error)?;

        // Record the transaction_id in the current tracing span
        let transaction_id = proof.id();
        tracing::Span::current().record("id", tracing::field::display(&transaction_id));

        Ok(Response::new(ProvingResponse { payload: proof.to_bytes() }))
    }

    #[instrument(
        target = MIDEN_PROVING_SERVICE,
        name = "proving_service:prove_batch",
        skip_all,
        ret(level = "debug"),
        fields(id = tracing::field::Empty),
        err
    )]
    pub fn prove_batch(
        &self,
        proposed_batch: ProposedBatch,
    ) -> Result<Response<ProvingResponse>, tonic::Status> {
        let prover = match &self.prover {
            Prover::Batch(prover) => prover,
            _ => return Err(Status::unimplemented("Batch prover is not enabled")),
        };

        let proven_batch = prover
            .try_lock()
            .map_err(|_| Status::resource_exhausted("Server is busy handling another request"))?
            .prove(proposed_batch)
            .map_err(internal_error)?;

        // Record the batch_id in the current tracing span
        let batch_id = proven_batch.id();
        tracing::Span::current().record("id", tracing::field::display(&batch_id));

        Ok(Response::new(ProvingResponse { payload: proven_batch.to_bytes() }))
    }

    #[instrument(
        target = MIDEN_PROVING_SERVICE,
        name = "proving_service:prove_block",
        skip_all,
        ret(level = "debug"),
        fields(id = tracing::field::Empty),
        err
    )]
    pub fn prove_block(
        &self,
        proposed_block: ProposedBlock,
    ) -> Result<Response<ProvingResponse>, tonic::Status> {
        let prover = match &self.prover {
            Prover::Block(prover) => prover,
            _ => return Err(Status::unimplemented("Block prover is not enabled")),
        };

        let proven_block = prover
            .try_lock()
            .map_err(|_| Status::resource_exhausted("Server is busy handling another request"))?
            .prove(proposed_block)
            .map_err(internal_error)?;

        // Record the commitment of the block in the current tracing span
        let block_id = proven_block.commitment();

        tracing::Span::current().record("id", tracing::field::display(&block_id));

        Ok(Response::new(ProvingResponse { payload: proven_block.to_bytes() }))
    }
}

#[async_trait::async_trait]
impl ProverApi for ProverRpcApi {
    #[instrument(
        target = MIDEN_PROVING_SERVICE,
        name = "proving_service:prove",
        skip_all,
        ret(level = "debug"),
        fields(id = tracing::field::Empty),
        err
    )]
    async fn prove(
        &self,
        request: Request<ProvingRequest>,
    ) -> Result<Response<ProvingResponse>, tonic::Status> {
        match request.get_ref().proof_type() {
            ProofType::Transaction => {
                let tx_witness = request.into_inner().try_into().map_err(invalid_argument)?;
                self.prove_tx(tx_witness)
            },
            ProofType::Batch => {
                let proposed_batch = request.into_inner().try_into().map_err(invalid_argument)?;
                self.prove_batch(proposed_batch)
            },
            ProofType::Block => {
                let proposed_block = request.into_inner().try_into().map_err(invalid_argument)?;
                self.prove_block(proposed_block)
            },
        }
    }
}

// UTILITIES
// ================================================================================================

/// Formats an error
fn internal_error<E: core::fmt::Debug>(err: E) -> Status {
    Status::internal(format!("{:?}", err))
}

fn invalid_argument<E: core::fmt::Debug>(err: E) -> Status {
    Status::invalid_argument(format!("{:?}", err))
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod test {
    use std::time::Duration;

    use miden_lib::transaction::TransactionKernel;
    use miden_objects::{
        asset::{Asset, FungibleAsset},
        note::NoteType,
        testing::{
            account_code::DEFAULT_AUTH_SCRIPT,
            account_id::{ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET, ACCOUNT_ID_SENDER},
        },
        transaction::{ProvenTransaction, TransactionScript, TransactionWitness},
    };
    use miden_testing::{Auth, MockChain};
    use miden_tx::utils::Serializable;
    use tokio::net::TcpListener;
    use tonic::Request;

    use crate::{
        api::{ProverRpcApi, prover::ProverType},
        generated::{ProofType, ProvingRequest, api_client::ApiClient, api_server::ApiServer},
    };

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn test_prove_transaction() {
        // Start the server in the background
        let listener = TcpListener::bind("127.0.0.1:50052").await.unwrap();

        let prover_type = ProverType::Transaction;

        let api_service = ApiServer::new(ProverRpcApi::new(prover_type));

        // Spawn the server as a background task
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .accept_http1(true)
                .add_service(tonic_web::enable(api_service))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        // Give the server some time to start
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Set up a gRPC client to send the request
        let mut client = ApiClient::connect("http://127.0.0.1:50052").await.unwrap();
        let mut client_2 = ApiClient::connect("http://127.0.0.1:50052").await.unwrap();

        // Create a mock transaction to send to the server
        let mut mock_chain = MockChain::new();
        let account = mock_chain.add_pending_existing_wallet(Auth::BasicAuth, vec![]);

        let fungible_asset_1: Asset =
            FungibleAsset::new(ACCOUNT_ID_PUBLIC_FUNGIBLE_FAUCET.try_into().unwrap(), 100)
                .unwrap()
                .into();
        let note_1 = mock_chain
            .add_pending_p2id_note(
                ACCOUNT_ID_SENDER.try_into().unwrap(),
                account.id(),
                &[fungible_asset_1],
                NoteType::Private,
                None,
            )
            .unwrap();

        let tx_script =
            TransactionScript::compile(DEFAULT_AUTH_SCRIPT, vec![], TransactionKernel::assembler())
                .unwrap();
        let tx_context = mock_chain
            .build_tx_context(account.id(), &[], &[])
            .input_notes(vec![note_1])
            .tx_script(tx_script)
            .build();

        let executed_transaction = tx_context.execute().unwrap();

        let transaction_witness = TransactionWitness::from(executed_transaction);

        let request_1 = Request::new(ProvingRequest {
            proof_type: ProofType::Transaction.into(),
            payload: transaction_witness.to_bytes(),
        });

        let request_2 = Request::new(ProvingRequest {
            proof_type: ProofType::Transaction.into(),
            payload: transaction_witness.to_bytes(),
        });

        // Send both requests concurrently
        let (t1, t2) = (
            tokio::spawn(async move { client.prove(request_1).await }),
            tokio::spawn(async move { client_2.prove(request_2).await }),
        );

        let (response_1, response_2) = (t1.await.unwrap(), t2.await.unwrap());

        // Check the success response
        assert!(response_1.is_ok() || response_2.is_ok());

        // Check the failure response
        assert!(response_1.is_err() || response_2.is_err());

        let response_success = response_1.or(response_2).unwrap();

        // Cast into a ProvenTransaction
        let _proven_transaction: ProvenTransaction =
            response_success.into_inner().try_into().expect("Failed to convert response");
    }
}

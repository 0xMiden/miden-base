use tonic::{Request, Response, Status};

use crate::{
    api::prover::ProverType,
    generated::status::{StatusRequest, StatusResponse, status_api_server::StatusApi},
};

pub struct StatusRpcApi {
    prover_type: ProverType,
}

impl StatusRpcApi {
    pub fn new(prover_type: ProverType) -> Self {
        Self { prover_type }
    }
}

#[async_trait::async_trait]
impl StatusApi for StatusRpcApi {
    async fn status(&self, _: Request<StatusRequest>) -> Result<Response<StatusResponse>, Status> {
        Ok(Response::new(StatusResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            supported_proof_type: self.prover_type.to_proof_type() as i32,
        }))
    }
}

use tokio::net::TcpListener;

use crate::generated::{api_server::ApiServer, status::status_api_server::StatusApiServer};

pub(crate) mod prover;
mod status;

pub use prover::{MIDEN_PROVING_SERVICE, ProverRpcApi, ProverType};

pub struct RpcListener {
    pub api_service: ApiServer<ProverRpcApi>,
    pub status_service: StatusApiServer<status::StatusRpcApi>,
    pub listener: TcpListener,
}

impl RpcListener {
    pub fn new(listener: TcpListener, prover_type: ProverType) -> Self {
        let prover_rpc_api = ProverRpcApi::new(prover_type);
        let status_rpc_api = status::StatusRpcApi::new(prover_type);
        let api_service = ApiServer::new(prover_rpc_api);
        let status_service = StatusApiServer::new(status_rpc_api);
        Self { listener, api_service, status_service }
    }
}

use std::sync::Arc;

use async_trait::async_trait;
use pingora::{server::ListenFds, services::Service};
use tokio::{net::TcpListener, sync::watch::Receiver};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request, Response, Status, transport::Server};
use tracing::{error, info};

use super::worker::WorkerHealthStatus as RustWorkerHealthStatus;
use crate::{
    commands::PROXY_HOST,
    generated::{
        proving_service::ProofType,
        proxy_status::{
            ProxyStatusRequest, ProxyStatusResponse, WorkerHealthStatus, WorkerStatus,
            proxy_status_api_server::{ProxyStatusApi, ProxyStatusApiServer},
        },
    },
    proxy::LoadBalancerState,
};

/// gRPC service that implements Pingora's Service trait
pub struct ProxyStatusService {
    load_balancer: Arc<LoadBalancerState>,
    port: u16,
}

/// Internal gRPC service implementation
pub struct ProxyStatusGrpcService {
    load_balancer: Arc<LoadBalancerState>,
}

impl ProxyStatusService {
    pub fn new(load_balancer: Arc<LoadBalancerState>, port: u16) -> Self {
        Self { load_balancer, port }
    }
}

impl ProxyStatusGrpcService {
    pub fn new(load_balancer: Arc<LoadBalancerState>) -> Self {
        Self { load_balancer }
    }
}

#[async_trait]
impl ProxyStatusApi for ProxyStatusGrpcService {
    async fn status(
        &self,
        _request: Request<ProxyStatusRequest>,
    ) -> Result<Response<ProxyStatusResponse>, Status> {
        let workers = self.load_balancer.workers.read().await;
        let worker_statuses: Vec<WorkerStatus> = workers
            .iter()
            .map(|w| WorkerStatus {
                address: w.address(),
                version: w.version().to_string(),
                status: WorkerHealthStatus::from(w.health_status()) as i32,
            })
            .collect();

        let supported_proof_type: ProofType = self.load_balancer.supported_prover_type.into();

        let response = ProxyStatusResponse {
            version: env!("CARGO_PKG_VERSION").to_string(),
            supported_proof_type: supported_proof_type as i32,
            workers: worker_statuses,
        };

        Ok(Response::new(response))
    }
}

#[async_trait]
impl Service for ProxyStatusService {
    async fn start_service(
        &mut self,
        #[cfg(unix)] _fds: Option<ListenFds>,

        mut shutdown: Receiver<bool>,
        _listeners_per_fd: usize,
    ) {
        info!("Starting gRPC status service on port {}", self.port);

        // Create a new listener
        let addr = format!("{}:{}", PROXY_HOST, self.port);
        let listener = match TcpListener::bind(&addr).await {
            Ok(listener) => {
                info!("gRPC status service bound to {}", addr);
                listener
            },
            Err(e) => {
                error!("Failed to bind gRPC status service to {}: {}", addr, e);
                return;
            },
        };

        // Create the gRPC service implementation
        let grpc_service = ProxyStatusGrpcService::new(self.load_balancer.clone());
        let status_server = ProxyStatusApiServer::new(grpc_service);

        // Build the tonic server
        let server = Server::builder().add_service(status_server).serve_with_incoming_shutdown(
            TcpListenerStream::new(listener),
            async move {
                let _ = shutdown.changed().await;
                info!("gRPC status service received shutdown signal");
            },
        );

        // Run the server
        if let Err(e) = server.await {
            error!("gRPC status service error: {}", e);
        } else {
            info!("gRPC status service stopped gracefully");
        }
    }

    fn name(&self) -> &str {
        "grpc-status"
    }

    fn threads(&self) -> Option<usize> {
        Some(1) // Single thread is sufficient for the status service
    }
}

impl From<&RustWorkerHealthStatus> for WorkerHealthStatus {
    fn from(status: &RustWorkerHealthStatus) -> Self {
        match status {
            RustWorkerHealthStatus::Healthy => WorkerHealthStatus::Healthy,
            RustWorkerHealthStatus::Unhealthy { .. } => WorkerHealthStatus::Unhealthy,
            RustWorkerHealthStatus::Unknown => WorkerHealthStatus::Unknown,
        }
    }
}

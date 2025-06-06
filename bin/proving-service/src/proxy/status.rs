use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use pingora::{server::ListenFds, services::Service};
use tokio::{
    net::TcpListener,
    sync::{RwLock, watch::Receiver},
    time::interval,
};
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

/// Update cache every 5 seconds
const CACHE_UPDATE_INTERVAL_SECS: u64 = 5;

/// Cached proxy status response
type CachedStatus = ProxyStatusResponse;

/// gRPC service that implements Pingora's Service trait
pub struct ProxyStatusService {
    load_balancer: Arc<LoadBalancerState>,
    port: u16,
    cache: Arc<RwLock<Option<CachedStatus>>>,
    update_interval: Duration,
}

/// Internal gRPC service implementation
pub struct ProxyStatusGrpcService {
    cache: Arc<RwLock<Option<CachedStatus>>>,
}

impl ProxyStatusService {
    pub fn new(load_balancer: Arc<LoadBalancerState>, port: u16) -> Self {
        let cache = Arc::new(RwLock::new(None));
        let update_interval = Duration::from_secs(CACHE_UPDATE_INTERVAL_SECS);
        Self {
            load_balancer,
            port,
            cache,
            update_interval,
        }
    }
}

async fn generate_and_store_status(
    load_balancer: &LoadBalancerState,
    cache: &RwLock<Option<CachedStatus>>,
) {
    let workers = load_balancer.workers.read().await;
    let worker_statuses: Vec<WorkerStatus> = workers
        .iter()
        .map(|w| WorkerStatus {
            address: w.address(),
            version: w.version().to_string(),
            status: WorkerHealthStatus::from(w.health_status()) as i32,
        })
        .collect();

    let supported_proof_type: ProofType = load_balancer.supported_prover_type.into();

    let status = ProxyStatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        supported_proof_type: supported_proof_type as i32,
        workers: worker_statuses,
    };

    let mut cache_write = cache.write().await;
    *cache_write = Some(status);
}

impl ProxyStatusGrpcService {
    fn new(cache: Arc<RwLock<Option<CachedStatus>>>) -> Self {
        Self { cache }
    }
}

#[async_trait]
impl ProxyStatusApi for ProxyStatusGrpcService {
    async fn status(
        &self,
        _request: Request<ProxyStatusRequest>,
    ) -> Result<Response<ProxyStatusResponse>, Status> {
        let cache_read = self.cache.read().await;
        if let Some(cached_response) = cache_read.as_ref() {
            Ok(Response::new(cached_response.clone()))
        } else {
            // This should not happen since we populate the cache on startup
            Err(Status::unavailable("Status not available yet"))
        }
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

        // Generate initial cache entry
        generate_and_store_status(&self.load_balancer, &self.cache).await;
        info!("Initial cache populated for gRPC status service");

        // Start the cache updater task
        let load_balancer = self.load_balancer.clone();
        let cache = self.cache.clone();
        let update_interval = self.update_interval;
        let mut cache_updater_shutdown = shutdown.clone();

        tokio::spawn(async move {
            let mut update_timer = interval(update_interval);

            loop {
                tokio::select! {
                    _ = update_timer.tick() => {
                        generate_and_store_status(&load_balancer, &cache).await;
                    }
                    _ = cache_updater_shutdown.changed() => {
                        info!("Cache updater received shutdown signal");
                        break;
                    }
                }
            }
        });

        // Create the gRPC service implementation
        let grpc_service = ProxyStatusGrpcService::new(self.cache.clone());
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

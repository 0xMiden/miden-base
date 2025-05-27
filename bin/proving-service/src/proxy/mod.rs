use std::{
    collections::VecDeque,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use bytes::Bytes;
use metrics::{
    QUEUE_LATENCY, QUEUE_SIZE, RATE_LIMIT_VIOLATIONS, RATE_LIMITED_REQUESTS, REQUEST_COUNT,
    REQUEST_FAILURE_COUNT, REQUEST_LATENCY, REQUEST_RETRIES, WORKER_BUSY, WORKER_COUNT,
    WORKER_REQUEST_COUNT,
};
use pingora::{
    http::ResponseHeader,
    prelude::*,
    protocols::Digest,
    upstreams::peer::{ALPN, Peer},
};
use pingora_core::{Result, upstreams::peer::HttpPeer};
use pingora_limits::rate::Rate;
use pingora_proxy::{FailToProxy, ProxyHttp, Session};
use tokio::sync::RwLock;
use tracing::{Span, debug, error, info, info_span, warn};
use uuid::Uuid;
use worker::Worker;

use crate::{
    commands::{
        ProxyConfig,
        update_workers::{Action, UpdateWorkers},
        worker::ProverType,
    },
    error::ProvingServiceError,
    utils::{
        MIDEN_PROVING_SERVICE, create_queue_full_response, create_response_with_error_message,
        create_too_many_requests_response,
    },
};

mod health_check;
pub mod metrics;
pub(crate) mod status;
pub(crate) mod update_workers;
mod worker;

// LOAD BALANCER STATE
// ================================================================================================

/// Load balancer that uses a round robin strategy
#[derive(Debug)]
pub struct LoadBalancerState {
    workers: Arc<RwLock<Vec<Worker>>>,
    timeout_secs: Duration,
    connection_timeout_secs: Duration,
    max_queue_items: usize,
    max_retries_per_request: usize,
    max_req_per_sec: isize,
    available_workers_polling_interval: Duration,
    health_check_interval: Duration,
    supported_prover_type: ProverType,
}

impl LoadBalancerState {
    /// Create a new load balancer
    ///
    /// # Errors
    /// Returns an error if:
    /// - The worker cannot be created.
    #[tracing::instrument(name = "proxy:new_load_balancer", skip(initial_workers))]
    pub async fn new(
        initial_workers: Vec<String>,
        config: &ProxyConfig,
    ) -> core::result::Result<Self, ProvingServiceError> {
        let mut workers: Vec<Worker> = Vec::with_capacity(initial_workers.len());

        let connection_timeout = Duration::from_secs(config.connection_timeout_secs);
        let total_timeout = Duration::from_secs(config.timeout_secs);

        for worker_addr in initial_workers {
            match Worker::new(worker_addr, connection_timeout, total_timeout).await {
                Ok(w) => workers.push(w),
                Err(e) => {
                    error!("Failed to create worker: {}", e);
                },
            }
        }

        info!("Workers created: {:?}", workers);

        WORKER_COUNT.set(workers.len() as i64);
        RATE_LIMIT_VIOLATIONS.reset();
        RATE_LIMITED_REQUESTS.reset();
        REQUEST_RETRIES.reset();

        Ok(Self {
            workers: Arc::new(RwLock::new(workers)),
            timeout_secs: total_timeout,
            connection_timeout_secs: connection_timeout,
            max_queue_items: config.max_queue_items,
            max_retries_per_request: config.max_retries_per_request,
            max_req_per_sec: config.max_req_per_sec,
            available_workers_polling_interval: Duration::from_millis(
                config.available_workers_polling_interval_ms,
            ),
            health_check_interval: Duration::from_secs(config.health_check_interval_secs),
            supported_prover_type: config.prover_type,
        })
    }

    /// Gets an available worker and marks it as unavailable.
    ///
    /// If no worker is available, it will return None.
    pub async fn pop_available_worker(&self) -> Option<Worker> {
        let mut available_workers = self.workers.write().await;
        available_workers.iter_mut().find(|w| w.is_available()).map(|w| {
            w.set_availability(false);
            WORKER_BUSY.inc();
            w.clone()
        })
    }

    /// Marks the given worker as available and moves it to the end of the list.
    ///
    /// If the worker is not in the list, it won't be added.
    /// The worker is moved to the end of the list to avoid overloading since the selection of the
    /// worker is done in order, causing the workers at the beginning of the list to be selected
    /// more often.
    pub async fn add_available_worker(&self, worker: Worker) {
        let mut workers = self.workers.write().await;
        if let Some(pos) = workers.iter().position(|w| *w == worker) {
            // Remove the worker from its current position
            let mut w = workers.remove(pos);
            // Mark it as available
            w.set_availability(true);
            // Add it to the end of the list
            workers.push(w);
        }
    }

    /// Updates the list of available workers based on the given action ("add" or "remove").
    ///
    /// # Behavior
    ///
    /// ## Add Action
    /// - If the worker exists in the current workers list, do nothing.
    /// - Otherwise, add it and mark it as available.
    ///
    /// ## Remove Action
    /// - If the worker exists in the current workers list, remove it.
    /// - Otherwise, do nothing.
    ///
    /// # Errors
    /// - If the worker cannot be created.
    pub async fn update_workers(
        &self,
        update_workers: UpdateWorkers,
    ) -> std::result::Result<(), ProvingServiceError> {
        let mut workers = self.workers.write().await;
        info!("Current workers: {:?}", workers);

        let mut native_workers = Vec::new();

        for worker_addr in update_workers.workers {
            native_workers.push(
                Worker::new(worker_addr, self.connection_timeout_secs, self.timeout_secs).await?,
            );
        }

        match update_workers.action {
            Action::Add => {
                for worker in native_workers {
                    if !workers.iter().any(|w| w == &worker) {
                        workers.push(worker);
                    }
                }
            },
            Action::Remove => {
                for worker in native_workers {
                    workers.retain(|w| w != &worker);
                }
            },
        }

        info!("Workers updated: {:?}", workers);
        WORKER_COUNT.set(workers.len() as i64);

        Ok(())
    }

    /// Get the total number of current workers.
    pub async fn num_workers(&self) -> usize {
        self.workers.read().await.len()
    }

    /// Get the number of busy workers.
    pub async fn num_busy_workers(&self) -> usize {
        self.workers.read().await.iter().filter(|w| !w.is_available()).count()
    }
}

/// Rate limiter
static RATE_LIMITER: LazyLock<Rate> = LazyLock::new(|| Rate::new(Duration::from_secs(1)));

// REQUEST QUEUE
// ================================================================================================

/// Request queue holds the list of requests that are waiting to be processed by the workers and
/// the time they were enqueued.
/// It is used to keep track of the order of the requests to then assign them to the workers.
pub struct RequestQueue {
    queue: RwLock<VecDeque<(Uuid, Instant)>>,
}

impl RequestQueue {
    /// Create a new empty request queue
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        QUEUE_SIZE.set(0);
        Self { queue: RwLock::new(VecDeque::new()) }
    }

    /// Get the length of the queue
    #[allow(clippy::len_without_is_empty)]
    pub async fn len(&self) -> usize {
        self.queue.read().await.len()
    }

    /// Enqueue a request
    pub async fn enqueue(&self, request_id: Uuid) {
        QUEUE_SIZE.inc();
        let mut queue = self.queue.write().await;
        queue.push_back((request_id, Instant::now()));
    }

    /// Dequeue a request
    pub async fn dequeue(&self) -> Option<Uuid> {
        let mut queue = self.queue.write().await;
        // If the queue was empty, the queue size does not change
        if let Some((request_id, queued_time)) = queue.pop_front() {
            QUEUE_SIZE.dec();
            QUEUE_LATENCY.observe(queued_time.elapsed().as_secs_f64());
            Some(request_id)
        } else {
            None
        }
    }

    /// Peek at the first request in the queue
    pub async fn peek(&self) -> Option<Uuid> {
        let queue = self.queue.read().await;
        queue.front().copied().map(|(request_id, _)| request_id)
    }
}

/// Shared state. It keeps track of the order of the requests to then assign them to the workers.
static QUEUE: LazyLock<RequestQueue> = LazyLock::new(RequestQueue::new);

// REQUEST CONTEXT
// ================================================================================================

/// Custom context for the request/response lifecycle
///
/// We use this context to keep track of the number of tries for a request, the unique ID for the
/// request, the worker that will process the request, a span that will be used for traces along
/// the transaction execution, and a timer to track how long the request took.
#[derive(Debug)]
pub struct RequestContext {
    /// Number of tries for the request
    tries: usize,
    /// Unique ID for the request
    request_id: Uuid,
    /// Worker that will process the request
    worker: Option<Worker>,
    /// Parent span for the request
    parent_span: Span,
    /// Time when the request was created
    created_at: Instant,
}

impl RequestContext {
    /// Create a new request context
    fn new() -> Self {
        let request_id = Uuid::new_v4();
        Self {
            tries: 0,
            request_id,
            worker: None,
            parent_span: info_span!(target: MIDEN_PROVING_SERVICE, "proxy:new_request", request_id = request_id.to_string()),
            created_at: Instant::now(),
        }
    }

    /// Set the worker that will process the request
    fn set_worker(&mut self, worker: Worker) {
        WORKER_REQUEST_COUNT.with_label_values(&[&worker.address()]).inc();
        self.worker = Some(worker);
    }
}

// LOAD BALANCER
// ================================================================================================

/// Wrapper around the load balancer that implements the ProxyHttp trait
///
/// This wrapper is used to implement the ProxyHttp trait for `Arc<LoadBalancer>`.
/// This is necessary because we want to share the load balancer between the proxy server and the
/// health check background service.
#[derive(Debug)]
pub struct LoadBalancer(pub Arc<LoadBalancerState>);

/// Implements load-balancing of incoming requests across a pool of workers.
///
/// At the backend-level, a request lifecycle works as follows:
/// - When a new requests arrives, [LoadBalancer::request_filter()] method is called. In this method
///   we apply IP-based rate-limiting to the request and check if the request queue is full. In this
///   method we also handle the special case update workers request.
/// - Next, the [Self::upstream_peer()] method is called. We use it to figure out which worker will
///   process the request. Inside `upstream_peer()`, we add the request to the queue of requests.
///   Once the request gets to the front of the queue, we forward it to an available worker. This
///   step is also in charge of setting the SNI, timeouts, and enabling HTTP/2. Finally, we
///   establish a connection with the worker.
/// - Before sending the request to the upstream server and if the connection succeed, the
///   [Self::upstream_request_filter()] method is called. In this method, we ensure that the correct
///   headers are forwarded for gRPC requests.
/// - If the connection fails, the [Self::fail_to_connect()] method is called. In this method, we
///   retry the request [self.max_retries_per_request] times.
/// - Once the worker processes the request (either successfully or with a failure),
///   [Self::logging()] method is called. In this method, we log the request lifecycle and set the
///   worker as available.
#[async_trait]
impl ProxyHttp for LoadBalancer {
    type CTX = RequestContext;
    fn new_ctx(&self) -> Self::CTX {
        RequestContext::new()
    }

    /// Decide whether to filter the request or not. Also, handle the special case of the update
    /// workers request.
    ///
    /// Here we apply IP-based rate-limiting to the request. We also check if the queue is full.
    ///
    /// If the request is rate-limited, we return a 429 response. Otherwise, we return false.
    #[tracing::instrument(name = "proxy:request_filter", parent = &ctx.parent_span, skip(session))]
    async fn request_filter(&self, session: &mut Session, ctx: &mut Self::CTX) -> Result<bool>
    where
        Self::CTX: Send + Sync,
    {
        // Extract the client address early
        let client_addr = match session.client_addr() {
            Some(addr) => addr.to_string(),
            None => {
                return create_response_with_error_message(
                    session.as_downstream_mut(),
                    "No socket address".to_string(),
                )
                .await;
            },
        };

        info!("Client address: {:?}", client_addr);

        // Increment the request count
        REQUEST_COUNT.inc();

        let user_id = Some(client_addr);

        // Retrieve the current window requests
        let curr_window_requests = RATE_LIMITER.observe(&user_id, 1);

        // Rate limit the request
        if curr_window_requests > self.0.max_req_per_sec {
            RATE_LIMITED_REQUESTS.inc();

            // Only count a violation the first time in a given window
            if curr_window_requests == self.0.max_req_per_sec + 1 {
                RATE_LIMIT_VIOLATIONS.inc();
            }

            return create_too_many_requests_response(session, self.0.max_req_per_sec).await;
        };

        let queue_len = QUEUE.len().await;

        info!("New request with ID: {}", ctx.request_id);
        info!("Queue length: {}", queue_len);

        // Check if the queue is full
        if queue_len >= self.0.max_queue_items {
            return create_queue_full_response(session).await;
        }

        Ok(false)
    }

    /// Returns [HttpPeer] corresponding to the worker that will handle the current request.
    ///
    /// Here we enqueue the request and wait for it to be at the front of the queue and a worker
    /// becomes available, then we dequeue the request and process it. We then set the SNI,
    /// timeouts, and enable HTTP/2.
    ///
    /// Note that the request will be assigned a worker here, and the worker will be removed from
    /// the list of available workers once it reaches the [Self::logging] method.
    #[tracing::instrument(name = "proxy:upstream_peer", parent = &ctx.parent_span, skip(_session))]
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        let request_id = ctx.request_id;

        // Add the request to the queue.
        QUEUE.enqueue(request_id).await;

        // Wait for the request to be at the front of the queue
        loop {
            // The request is at the front of the queue.
            if QUEUE.peek().await.expect("Queue should not be empty") != request_id {
                continue;
            }

            // Check if there is an available worker
            if let Some(worker) = self.0.pop_available_worker().await {
                debug!("Worker {} picked up the request with ID: {}", worker.address(), request_id);
                ctx.set_worker(worker);
                break;
            }
            debug!("All workers are busy");
            tokio::time::sleep(self.0.available_workers_polling_interval).await;
        }

        // Remove the request from the queue
        QUEUE.dequeue().await;

        // Set SNI
        let mut http_peer = HttpPeer::new(
            ctx.worker.clone().expect("Failed to get worker").address(),
            false,
            "".to_string(),
        );
        let peer_opts =
            http_peer.get_mut_peer_options().ok_or(Error::new(ErrorType::InternalError))?;

        // Timeout settings
        peer_opts.total_connection_timeout = Some(self.0.timeout_secs);
        peer_opts.connection_timeout = Some(self.0.connection_timeout_secs);
        peer_opts.read_timeout = Some(self.0.timeout_secs);
        peer_opts.write_timeout = Some(self.0.timeout_secs);
        peer_opts.idle_timeout = Some(self.0.timeout_secs);

        // Enable HTTP/2
        peer_opts.alpn = ALPN::H2;

        let peer = Box::new(http_peer);
        Ok(peer)
    }

    /// Applies the necessary filters to the request before sending it to the upstream server.
    ///
    /// Here we ensure that the correct headers are forwarded for gRPC requests.
    ///
    /// This method is called right after [Self::upstream_peer()] returns a [HttpPeer] and a
    /// connection is established with the worker.
    #[tracing::instrument(name = "proxy:upstream_request_filter", parent = &_ctx.parent_span, skip(_session))]
    async fn upstream_request_filter(
        &self,
        _session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut Self::CTX,
    ) -> Result<()>
    where
        Self::CTX: Send + Sync,
    {
        // Check if it's a gRPC request
        if let Some(content_type) = upstream_request.headers.get("content-type") {
            if content_type == "application/grpc" {
                // Ensure the correct host and gRPC headers are forwarded
                upstream_request.insert_header("content-type", "application/grpc")?;
            }
        }

        Ok(())
    }

    /// Retry the request if the connection fails.
    #[tracing::instrument(name = "proxy:fail_to_connect", parent = &ctx.parent_span, skip(_session))]
    fn fail_to_connect(
        &self,
        _session: &mut Session,
        _peer: &HttpPeer,
        ctx: &mut Self::CTX,
        mut e: Box<Error>,
    ) -> Box<Error> {
        if ctx.tries > self.0.max_retries_per_request {
            return e;
        }
        REQUEST_RETRIES.inc();
        ctx.tries += 1;
        e.set_retry(true);
        e
    }

    /// Logs the request lifecycle in case that an error happened and sets the worker as available.
    ///
    /// This method is the last one in the request lifecycle, no matter if the request was
    /// processed or not.
    #[tracing::instrument(name = "proxy:logging", parent = &ctx.parent_span, skip(_session))]
    async fn logging(&self, _session: &mut Session, e: Option<&Error>, ctx: &mut Self::CTX)
    where
        Self::CTX: Send + Sync,
    {
        if let Some(e) = e {
            REQUEST_FAILURE_COUNT.inc();
            error!("Error: {:?}", e);
        }

        // Mark the worker as available
        if let Some(worker) = ctx.worker.take() {
            self.0.add_available_worker(worker).await;
        }

        REQUEST_LATENCY.observe(ctx.created_at.elapsed().as_secs_f64());

        // Update the number of busy workers
        WORKER_BUSY.set(self.0.num_busy_workers().await as i64);
    }

    // The following methods are a copy of the default implementation defined in the trait, but
    // with tracing instrumentation.
    // Pingora calls these methods to handle the request/response lifecycle internally and since
    // the trait is defined in a different crate, we cannot add the tracing instrumentation there.
    // We use the default implementation by implementing the method for our specific type, adding
    // the tracing instrumentation and internally calling `ProxyHttp` methods.
    // ============================================================================================
    #[tracing::instrument(name = "proxy:early_request_filter", parent = &ctx.parent_span, skip(_session))]
    async fn early_request_filter(
        &self,
        _session: &mut Session,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        ProxyHttpDefaultImpl.early_request_filter(_session, &mut ()).await
    }

    #[tracing::instrument(name = "proxy:connected_to_upstream", parent = &ctx.parent_span, skip(_session, _sock, _reused, _peer, _fd, _digest))]
    async fn connected_to_upstream(
        &self,
        _session: &mut Session,
        _reused: bool,
        _peer: &HttpPeer,
        #[cfg(unix)] _fd: std::os::unix::io::RawFd,
        #[cfg(windows)] _sock: std::os::windows::io::RawSocket,
        _digest: Option<&Digest>,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        ProxyHttpDefaultImpl
            .connected_to_upstream(_session, _reused, _peer, _fd, _digest, &mut ())
            .await
    }

    #[tracing::instrument(name = "proxy:request_body_filter", parent = &ctx.parent_span, skip(_session, _body))]
    async fn request_body_filter(
        &self,
        _session: &mut Session,
        _body: &mut Option<Bytes>,
        _end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        ProxyHttpDefaultImpl
            .request_body_filter(_session, _body, _end_of_stream, &mut ())
            .await
    }

    #[tracing::instrument(name = "proxy:upstream_response_filter", parent = &ctx.parent_span, skip(_session, _upstream_response))]
    fn upstream_response_filter(
        &self,
        _session: &mut Session,
        _upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        ProxyHttpDefaultImpl.upstream_response_filter(_session, _upstream_response, &mut ())
    }

    #[tracing::instrument(name = "proxy:response_filter", parent = &ctx.parent_span, skip(_session, _upstream_response))]
    async fn response_filter(
        &self,
        _session: &mut Session,
        _upstream_response: &mut ResponseHeader,
        ctx: &mut Self::CTX,
    ) -> Result<()>
    where
        Self::CTX: Send + Sync,
    {
        ProxyHttpDefaultImpl
            .response_filter(_session, _upstream_response, &mut ())
            .await
    }

    #[tracing::instrument(name = "proxy:upstream_response_body_filter", parent = &ctx.parent_span, skip(_session, _body))]
    fn upstream_response_body_filter(
        &self,
        _session: &mut Session,
        _body: &mut Option<Bytes>,
        _end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<()> {
        ProxyHttpDefaultImpl.upstream_response_body_filter(_session, _body, _end_of_stream, &mut ())
    }

    #[tracing::instrument(name = "proxy:response_body_filter", parent = &ctx.parent_span, skip(_session, _body))]
    fn response_body_filter(
        &self,
        _session: &mut Session,
        _body: &mut Option<Bytes>,
        _end_of_stream: bool,
        ctx: &mut Self::CTX,
    ) -> Result<Option<Duration>>
    where
        Self::CTX: Send + Sync,
    {
        ProxyHttpDefaultImpl.response_body_filter(_session, _body, _end_of_stream, &mut ())
    }

    #[tracing::instrument(name = "proxy:fail_to_proxy", parent = &ctx.parent_span, skip(session))]
    async fn fail_to_proxy(
        &self,
        session: &mut Session,
        e: &Error,
        ctx: &mut Self::CTX,
    ) -> FailToProxy
    where
        Self::CTX: Send + Sync,
    {
        ProxyHttpDefaultImpl.fail_to_proxy(session, e, &mut ()).await
    }

    #[tracing::instrument(name = "proxy:error_while_proxy", parent = &ctx.parent_span, skip(session))]
    fn error_while_proxy(
        &self,
        peer: &HttpPeer,
        session: &mut Session,
        e: Box<Error>,
        ctx: &mut Self::CTX,
        client_reused: bool,
    ) -> Box<Error> {
        ProxyHttpDefaultImpl.error_while_proxy(peer, session, e, &mut (), client_reused)
    }
}

// PROXY HTTP DEFAULT IMPLEMENTATION
// ================================================================================================

/// Default implementation of the [ProxyHttp] trait.
///
/// It is used to provide the default methods of the trait in order for the [LoadBalancer] to
/// implement the trait adding tracing instrumentation but without having to copy all default
/// implementations.
struct ProxyHttpDefaultImpl;

#[async_trait]
impl ProxyHttp for ProxyHttpDefaultImpl {
    type CTX = ();
    fn new_ctx(&self) {}

    /// This method is the only one that does not have a default implementation in the trait.
    async fn upstream_peer(
        &self,
        _session: &mut Session,
        _ctx: &mut Self::CTX,
    ) -> Result<Box<HttpPeer>> {
        unimplemented!("This is a dummy implementation, should not be called")
    }
}

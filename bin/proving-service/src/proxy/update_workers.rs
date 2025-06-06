use core::fmt;
use std::sync::Arc;

use miden_proving_service::api::MIDEN_PROVING_SERVICE;
use pingora::{
    apps::{HttpServerApp, HttpServerOptions},
    http::ResponseHeader,
    protocols::{Stream, http::ServerSession},
    server::ShutdownWatch,
};
use tonic::async_trait;
use tracing::{error, info};

use super::LoadBalancerState;
use crate::{commands::update_workers::UpdateWorkers, proxy::create_response_with_error_message};

/// The Load Balancer Updater Service.
///
/// This service is responsible for updating the list of workers in the load balancer.
pub(crate) struct LoadBalancerUpdateService {
    lb_state: Arc<LoadBalancerState>,
    server_opts: HttpServerOptions,
}

/// Manually implement Debug for LoadBalancerUpdateService.
/// [HttpServerOptions] does not implement Debug, so we cannot derive Debug for
/// [LoadBalancerUpdateService], which is needed for the tracing instrumentation.
impl fmt::Debug for LoadBalancerUpdateService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LBUpdaterService").field("lb_state", &self.lb_state).finish()
    }
}

impl LoadBalancerUpdateService {
    pub(crate) fn new(lb_state: Arc<LoadBalancerState>) -> Self {
        let mut server_opts = HttpServerOptions::default();
        server_opts.h2c = true;

        Self { lb_state, server_opts }
    }
}

#[async_trait]
impl HttpServerApp for LoadBalancerUpdateService {
    /// Handles the update workers request.
    ///
    /// # Behavior
    /// - Reads the HTTP request from the session.
    /// - If query parameters are present, attempts to parse them as an `UpdateWorkers` object.
    /// - If the parsing fails, returns an error response.
    /// - If successful, updates the list of workers by calling `update_workers`.
    /// - If the update is successful, returns the count of available workers.
    ///
    /// # Errors
    /// - If the HTTP request cannot be read.
    /// - If the query parameters cannot be parsed.
    /// - If the workers cannot be updated.
    /// - If the response cannot be created.
    #[tracing::instrument(target = MIDEN_PROVING_SERVICE, name = "lb_updater_service:process_new_http", skip(http))]
    async fn process_new_http(
        self: &Arc<Self>,
        mut http: ServerSession,
        _shutdown: &ShutdownWatch,
    ) -> Option<Stream> {
        match http.read_request().await {
            Ok(res) => {
                if !res {
                    error!("Failed to read request header");
                    create_response_with_error_message(
                        &mut http,
                        "Failed to read request header".to_string(),
                    )
                    .await
                    .ok();
                    return None;
                }
            },
            Err(e) => {
                error!("HTTP server fails to read from downstream: {e}");
                create_response_with_error_message(
                    &mut http,
                    format!("HTTP server fails to read from downstream: {e}"),
                )
                .await
                .ok();
                return None;
            },
        }

        info!("Successfully get a new request to update workers");

        // Extract and parse query parameters, if there are not any, return early.
        let query_params = match http.req_header().as_ref().uri.query() {
            Some(params) => params,
            None => {
                let error_message = "No query parameters provided".to_string();
                error!("{}", error_message);
                create_response_with_error_message(&mut http, error_message).await.ok();
                return None;
            },
        };

        let update_workers: Result<UpdateWorkers, _> = serde_qs::from_str(query_params);
        let update_workers = match update_workers {
            Ok(workers) => workers,
            Err(err) => {
                let error_message = format!("Failed to parse query parameters: {}", err);
                error!("{}", error_message);
                create_response_with_error_message(&mut http, error_message).await.ok();
                return None;
            },
        };

        // Update workers and handle potential errors
        if let Err(err) = self.lb_state.update_workers(update_workers).await {
            let error_message = format!("Failed to update workers: {}", err);
            error!("{}", error_message);
            create_response_with_error_message(&mut http, error_message).await.ok();
            return None;
        }

        create_workers_updated_response(&mut http, self.lb_state.num_workers().await)
            .await
            .ok();

        info!("Successfully updated workers");

        None
    }

    /// Provide HTTP server options used to override default behavior. This function will be called
    /// every time a new connection is processed.
    fn server_options(&self) -> Option<&HttpServerOptions> {
        Some(&self.server_opts)
    }
}

// HELPERS
// ================================================================================================

/// Create a 200 response for updated workers
///
/// It will set the X-Worker-Count header to the number of workers.
async fn create_workers_updated_response(
    session: &mut ServerSession,
    workers: usize,
) -> pingora_core::Result<bool> {
    let mut header = ResponseHeader::build(200, None)?;
    header.insert_header("X-Worker-Count", workers.to_string())?;
    session.set_keepalive(None);
    session.write_response_header(Box::new(header)).await?;
    Ok(true)
}

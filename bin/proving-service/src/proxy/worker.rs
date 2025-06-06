use std::{
    sync::LazyLock,
    time::{Duration, Instant},
};

use miden_proving_service::{
    api::ProverType,
    error::ProvingServiceError,
    generated::status::{StatusRequest, status_api_client::StatusApiClient},
};
use pingora::lb::Backend;
use semver::{Version, VersionReq};
use serde::Serialize;
use tonic::transport::Channel;
use tracing::error;

use super::metrics::WORKER_UNHEALTHY;

/// The maximum exponent for the backoff.
///
/// The maximum backoff is 2^[MAX_BACKOFF_EXPONENT] seconds.
const MAX_BACKOFF_EXPONENT: usize = 9;

/// The version of the proxy.
///
/// This is the version of the proxy that is used to check the version of the worker.
const MPS_PROXY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The version requirement for the worker.
///
/// This is the version requirement for the worker that is used to check the version of the worker.
static WORKER_VERSION_REQUIREMENT: LazyLock<VersionReq> = LazyLock::new(|| {
    let current =
        Version::parse(MPS_PROXY_VERSION).expect("Proxy version should be valid at this point");
    VersionReq::parse(&format!("~{}.{}", current.major, current.minor))
        .expect("Version should be valid at this point")
});

// WORKER
// ================================================================================================

/// A worker used for processing of requests.
///
/// A worker consists of a backend service (defined by worker address), a flag indicating wheter
/// the worker is currently available to process new requests, and a gRPC status client.
#[derive(Debug, Clone)]
pub struct Worker {
    backend: Backend,
    status_client: StatusApiClient<Channel>,
    is_available: bool,
    health_status: WorkerHealthStatus,
    version: String,
}

/// The health status of a worker.
///
/// A worker can be either healthy or unhealthy.
/// If the worker is unhealthy, it will have a number of failed attempts.
/// The number of failed attempts is incremented each time the worker is unhealthy.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum WorkerHealthStatus {
    /// The worker is healthy.
    Healthy,
    /// The worker is unhealthy.
    Unhealthy {
        /// The number of failed attempts.
        num_failed_attempts: usize,
        /// The timestamp of the first failure.
        #[serde(skip_serializing)]
        first_fail_timestamp: Instant,
        /// The reason for the failure.
        reason: String,
    },
    /// The worker status is unknown.
    Unknown,
}

impl Worker {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------

    /// Creates a new worker and a gRPC status client for the given worker address.
    ///
    /// # Errors
    /// - Returns [ProvingServiceError::InvalidURI] if the worker address is invalid.
    /// - Returns [ProvingServiceError::ConnectionFailed] if the connection to the worker fails.
    pub async fn new(
        worker_addr: String,
        connection_timeout: Duration,
        total_timeout: Duration,
    ) -> Result<Self, ProvingServiceError> {
        let backend =
            Backend::new(&worker_addr).map_err(ProvingServiceError::BackendCreationFailed)?;

        let status_client =
            create_status_client(worker_addr, connection_timeout, total_timeout).await?;

        Ok(Self {
            backend,
            is_available: true,
            status_client,
            health_status: WorkerHealthStatus::Unknown,
            version: "".to_string(),
        })
    }

    // MUTATORS
    // --------------------------------------------------------------------------------------------

    /// Checks the current status of the worker and marks the worker as healthy or unhealthy based
    /// on the status.
    ///
    /// If the worker is unhealthy, it will be marked as unavailable thus preventing requests from
    /// being sent to it. If a previously unhealthy worker becomes healthy, it will be marked as
    /// available and the proxy will start sending incoming requests to it.
    pub async fn check_status(&mut self, supported_prover_type: ProverType) {
        if !self.should_do_health_check() {
            return;
        }

        let failed_attempts = self.num_failures();

        let worker_status = match self.status_client.status(StatusRequest {}).await {
            Ok(response) => response.into_inner(),
            Err(e) => {
                error!("Failed to check worker status ({}): {}", self.address(), e);
                self.set_health_status(WorkerHealthStatus::Unhealthy {
                    num_failed_attempts: failed_attempts + 1,
                    first_fail_timestamp: Instant::now(),
                    reason: e.message().to_string(),
                });
                return;
            },
        };

        if worker_status.version.is_empty() {
            self.set_health_status(WorkerHealthStatus::Unhealthy {
                num_failed_attempts: failed_attempts + 1,
                first_fail_timestamp: Instant::now(),
                reason: "Worker version is empty".to_string(),
            });
            return;
        }

        if !is_valid_version(&WORKER_VERSION_REQUIREMENT, &worker_status.version).unwrap_or(false) {
            self.set_health_status(WorkerHealthStatus::Unhealthy {
                num_failed_attempts: failed_attempts + 1,
                first_fail_timestamp: Instant::now(),
                reason: format!("Worker version is invalid ({})", worker_status.version),
            });
            return;
        }

        self.version = worker_status.version;

        let worker_supported_proof_type =
            match ProverType::try_from(worker_status.supported_proof_type) {
                Ok(proof_type) => proof_type,
                Err(e) => {
                    error!(
                        "Failed to convert worker supported proof type ({}): {}",
                        self.address(),
                        e
                    );
                    self.set_health_status(WorkerHealthStatus::Unhealthy {
                        num_failed_attempts: failed_attempts + 1,
                        first_fail_timestamp: Instant::now(),
                        reason: e.to_string(),
                    });
                    return;
                },
            };

        if supported_prover_type != worker_supported_proof_type {
            self.set_health_status(WorkerHealthStatus::Unhealthy {
                num_failed_attempts: failed_attempts + 1,
                first_fail_timestamp: Instant::now(),
                reason: format!("Unsupported prover type: {}", worker_supported_proof_type),
            });
            return;
        }

        self.set_health_status(WorkerHealthStatus::Healthy);
    }

    /// Sets the worker availability.
    pub fn set_availability(&mut self, is_available: bool) {
        self.is_available = is_available
    }

    // PUBLIC ACCESSORS
    // --------------------------------------------------------------------------------------------

    /// Returns the number of failures the worker has had.
    pub fn num_failures(&self) -> usize {
        match &self.health_status {
            WorkerHealthStatus::Healthy => 0,
            WorkerHealthStatus::Unhealthy {
                num_failed_attempts: failed_attempts,
                first_fail_timestamp: _,
                reason: _,
            } => *failed_attempts,
            WorkerHealthStatus::Unknown => 0,
        }
    }

    /// Returns the health status of the worker.
    pub fn health_status(&self) -> &WorkerHealthStatus {
        &self.health_status
    }

    /// Returns the version of the worker.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the worker availability.
    ///
    /// A worker is available if it is healthy and ready to process requests.
    pub fn is_available(&self) -> bool {
        self.is_available
    }

    /// Returns the worker address.
    pub fn address(&self) -> String {
        self.backend.addr.to_string()
    }

    /// Returns whether the worker is healthy.
    ///
    /// This function will return `true` if the worker is healthy or the health status is unknown.
    /// Otherwise, it will return `false`.
    pub fn is_healthy(&self) -> bool {
        !matches!(self.health_status, WorkerHealthStatus::Unhealthy { .. })
    }

    // PRIVATE HELPERS
    // --------------------------------------------------------------------------------------------

    /// Returns whether the worker should do a health check.
    ///
    /// A worker should do a health check if it is healthy or if the time since the first failure
    /// is greater than the time since the first failure power of 2.
    ///
    /// The maximum exponent is [MAX_BACKOFF_EXPONENT], which corresponds to a backoff of
    /// 2^[MAX_BACKOFF_EXPONENT] seconds.
    fn should_do_health_check(&self) -> bool {
        match self.health_status {
            WorkerHealthStatus::Healthy => true,
            WorkerHealthStatus::Unhealthy {
                num_failed_attempts: failed_attempts,
                first_fail_timestamp,
                reason: _,
            } => {
                let time_since_first_failure = Instant::now() - first_fail_timestamp;
                time_since_first_failure
                    > Duration::from_secs(
                        2u64.pow(failed_attempts.min(MAX_BACKOFF_EXPONENT) as u32),
                    )
            },
            WorkerHealthStatus::Unknown => true,
        }
    }

    /// Sets the health status of the worker.
    ///
    /// This function will update the health status of the worker and update the worker availability
    /// based on the new health status.
    fn set_health_status(&mut self, health_status: WorkerHealthStatus) {
        let was_healthy = self.is_healthy();
        self.health_status = health_status;
        match &self.health_status {
            WorkerHealthStatus::Healthy => {
                if !was_healthy {
                    self.is_available = true;
                }
            },
            WorkerHealthStatus::Unhealthy { .. } => {
                WORKER_UNHEALTHY.with_label_values(&[&self.address()]).inc();
                self.is_available = false;
            },
            WorkerHealthStatus::Unknown => {
                if !was_healthy {
                    self.is_available = true;
                }
            },
        }
    }
}

// PARTIAL EQUALITY
// ================================================================================================

impl PartialEq for Worker {
    fn eq(&self, other: &Self) -> bool {
        self.backend == other.backend
    }
}

// HELPER FUNCTIONS
// ================================================================================================

/// Create a gRPC [StatusApiClient] for the given worker address.
///
/// # Errors
/// - [ProvingServiceError::InvalidURI] if the worker address is invalid.
/// - [ProvingServiceError::ConnectionFailed] if the connection to the worker fails.
async fn create_status_client(
    address: String,
    connection_timeout: Duration,
    total_timeout: Duration,
) -> Result<StatusApiClient<Channel>, ProvingServiceError> {
    let channel = Channel::from_shared(format!("http://{}", address))
        .map_err(|err| ProvingServiceError::InvalidURI(err, address.clone()))?
        .connect_timeout(connection_timeout)
        .timeout(total_timeout)
        .connect()
        .await
        .map_err(|err| ProvingServiceError::ConnectionFailed(err, address))?;

    Ok(StatusApiClient::new(channel))
}

/// Returns true if the version has major and minor versions match the major and minor
/// versions of the required version. Returns false otherwise.
///
/// # Errors
/// Returns an error if either of the versions is malformed.
fn is_valid_version(version_req: &VersionReq, version: &str) -> Result<bool, String> {
    let received =
        Version::parse(version).map_err(|err| format!("Invalid worker version: {err}"))?;

    Ok(version_req.matches(&received))
}

// TESTS
// ================================================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_valid_version() {
        let version_req = VersionReq::parse("~1.0").unwrap();
        assert!(is_valid_version(&version_req, "1.0.0").unwrap());
        assert!(is_valid_version(&version_req, "1.0.1").unwrap());
        assert!(is_valid_version(&version_req, "1.0.12").unwrap());
        assert!(is_valid_version(&version_req, "1.0").is_err());
        assert!(!is_valid_version(&version_req, "2.0.0").unwrap());
        assert!(!is_valid_version(&version_req, "1.1.0").unwrap());
        assert!(!is_valid_version(&version_req, "0.9.0").unwrap());
        assert!(!is_valid_version(&version_req, "0.9.1").unwrap());
        assert!(!is_valid_version(&version_req, "0.10.0").unwrap());
        assert!(is_valid_version(&version_req, "miden").is_err());
        assert!(is_valid_version(&version_req, "1.miden.12").is_err());
    }
}

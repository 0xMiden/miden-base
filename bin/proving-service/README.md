# Miden proving service

A service for generating Miden proofs on-demand. The binary enables spawning workers and a proxy for Miden's remote proving service. It currently supports proving individual transactions, transaction batches, and blocks.

A worker is a gRPC service that can receive transaction witnesses, proposed batches, or proposed blocks, prove them, and return the generated proofs. It can handle only one request at a time and will return an error if it is already in use. Each worker is specialized on startup to handle exactly one type of proof requests - transactions, batches, or blocks.

The proxy uses [Cloudflare's Pingora crate](https://crates.io/crates/pingora), which provides features to create a modular proxy. It is meant to handle multiple workers with a queue, assigning a worker to each request and retrying if the worker is not available. Further information about Pingora and its features can be found in the [official GitHub repository](https://github.com/cloudflare/pingora).


## Debian Installation

#### Prover

Install the Debian package:
```bash
set -e

sudo wget https://github.com/0xMiden/miden-base/releases/download/v0.8/miden-prover-v0.8-arm64.deb -O prover.deb
sudo wget -q -O - https://github.com/0xMiden/miden-base/releases/download/v0.8/miden-prover-v0.8-arm64.deb.checksum | awk '{print $1}' | sudo tee prover.checksum
sudo sha256sum prover.deb | awk '{print $1}' > prover.sha256
sudo diff prover.sha256 prover.checksum
sudo dpkg -i prover.deb
sudo rm prover.deb
```

Edit the configuration file `/lib/systemd/system/miden-prover.service.env`

Run the service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable miden-prover
sudo systemctl start miden-prover
```

#### Prover Proxy
```bash
set -e

sudo wget https://github.com/0xMiden/miden-base/releases/download/v0.8/miden-prover-proxy-v0.8-arm64.deb -O prover-proxy.deb
sudo wget -q -O - https://github.com/0xMiden/miden-base/releases/download/v0.8/miden-prover-proxy-v0.8-arm64.deb.checksum | awk '{print $1}' | sudo tee prover-proxy.checksum
sudo sha256sum prover-proxy.deb | awk '{print $1}' > prover-proxy.sha256
sudo diff prover-proxy.sha256 prover-proxy.checksum
sudo dpkg -i prover-proxy.deb
sudo rm prover-proxy.deb
```

Edit the configuration file `/lib/systemd/system/miden-prover-proxy.service.env`

Edit the service file to specify workers `/lib/systemd/system/miden-prover-proxy.service`

Run the service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable miden-prover-proxy
sudo systemctl start miden-prover-proxy
```

## Source Installation

To build the service from a local version, from the root of the workspace you can run:

```bash
make install-proving-service
```

The CLI can be installed from the source code using specific git revisions with `cargo install` or from crates.io with `cargo install miden-proving-service`.

## Worker

To start the worker service you will need to run:

```bash
miden-proving-service start-worker --port 8082 --prover-type transaction
```

This will spawn a worker using the port defined in the command option. The host will be 0.0.0.0 by default, or 127.0.0.1 if the --localhost flag is used. In case that the port is not provided, it will default to `50051`. This command will start a worker that can handle transaction and batch proving requests.

The `--prover-type` flag is required and specifies which type of proof the worker will handle. The available options are:
- `transaction`: For transaction proofs
- `batch`: For batch proofs
- `block`: For block proofs

Each worker can only handle one type of proof. If you need to handle multiple proof types, you should start multiple workers, each with a different proof type. Additionally, you can use the `--localhost` flag to bind to 127.0.0.1 instead of 0.0.0.0.

### Worker Configuration

The worker can be configured using the following environment variables:

| Variable                  | Description                     | Default       |
|---------------------------|---------------------------------|---------------|
| `MPS_WORKER_LOCALHOST`    | Use localhost (127.0.0.1)       | `false`       |
| `MPS_WORKER_PORT`         | The port number for the worker  | `50051`       |
| `MPS_WORKER_PROVER_TYPE`  | The supported prover type       | `transaction` |

For example:
```bash
export MPS_WORKER_LOCALHOST="true"
export MPS_WORKER_PORT="8082"
export MPS_WORKER_PROVER_TYPE="block"
miden-proving-service start-worker
```

## Proxy

To start the proxy service, you will need to run:

```bash
miden-proving-service start-proxy --prover-type transaction --workers [worker1],[worker2],...,[workerN]
```

For example:

```bash
miden-proving-service start-proxy --prover-type transaction --workers 0.0.0.0:8084,0.0.0.0:8085
```

This command will start the proxy using the workers passed as arguments. The workers should be in the format `host:port`. Another way to specify the workers is by using the `MPS_PROXY_WORKERS_LIST` environment variable, which can be set to a comma-separated list of worker addresses. For example:

```bash
export MPS_PROXY_WORKERS_LIST="0.0.0.0:8084,0.0.0.0:8085"
```

If no workers are passed, the proxy will start without any workers and will not be able to handle any requests until one is added through the `miden-proving-service add-worker` command.

The `--prover-type` flag is required and specifies which type of proof the proxy will handle. The available options are:
- `transaction`: For transaction proofs
- `batch`: For batch proofs
- `block`: For block proofs

The proxy can only handle one type of proof at a time. When you add workers to the proxy, it will check their supported proof type. Workers that support a different proof type than the proxy will be marked as unhealthy and will not be used for proving requests.

For example, if you start a proxy with `--prover-type transaction` and add these workers:
- Worker 1: Transaction proofs (Healthy)
- Worker 2: Batch proofs (Unhealthy - incompatible proof type)
- Worker 3: Block proofs (Unhealthy - incompatible proof type)

Only Worker 1 will be used for proving requests, while Workers 2 and 3 will be marked as unhealthy due to incompatible proof types.

You can customize the proxy service by setting environment variables. Possible customizations can be found by running `miden-proving-service start-proxy --help`.

An example `.env` file is provided in the crate's root directory. To use the variables from a file in any Unix-like operating system, you can run `source <your-file>`.

At the moment, when a worker added to the proxy stops working and can not connect to it for a request, the connection is marked as retriable meaning that the proxy will try reaching another worker. The number of retries is configurable via the `MPS_MAX_RETRIES_PER_REQUEST` environmental variable.

## Updating workers on a running proxy

To update the workers on a running proxy, two commands are provided: `add-workers` and `remove-workers`. These commands will update the workers on the proxy and will not require a restart. To use these commands, you will need to run:

```bash
miden-proving-service add-workers --control-port <port> [worker1],[worker2],...,[workerN]
miden-proving-service remove-workers --control-port <port> [worker1],[worker2],...,[workerN]
```
For example:

```bash
# To add 0.0.0.0:8085 and 200.58.70.4:50051 to the workers list:
miden-proving-service add-workers --control-port 8083 0.0.0.0:8085,200.58.70.4:50051
# To remove 158.12.12.3:8080 and 122.122.6.6:50051 from the workers list:
miden-proving-service remove-workers --control-port 8083 158.12.12.3:8080,122.122.6.6:50051
```

These commands can receive the list of workers to update as a comma-separated list of addresses through the `MPS_PROXY_WORKERS_LIST` environment variable, or as command-line arguments:

```bash
export MPS_PROXY_WORKERS_LIST="0.0.0.0:8085,200.58.70.4:50051"
miden-proving-service add-workers --control-port 8083
miden-proving-service remove-workers --control-port 8083
```

The `--control-port` flag is required to specify the port where the proxy is listening for updates. The workers are passed as arguments in the format `host:port`. The port can be specified via the `MPS_CONTROL_PORT` environment variable. For example:

```bash
export MPS_CONTROL_PORT="8083"
miden-proving-service add-workers 0.0.0.0:8085
```

Note that, in order to update the workers, the proxy must be running in the same computer as the command is being executed because it will check if the client address is localhost to avoid any security issues.

### Health check

The worker service implements the [gRPC Health Check](https://grpc.io/docs/guides/health-checking/) standard, and includes the methods described in this [official proto file](https://github.com/grpc/grpc-proto/blob/master/grpc/health/v1/health.proto).

The proxy service uses this health check to determine if a worker is available to receive requests. If a worker is not available, it will be removed from the set of workers that the proxy can use to send requests.

### Status check

The worker service implements a custom status check that returns information about the worker's current state and supported proof type. The proxy service uses this status check to determine if a worker is available to receive requests and if it supports the required proof type. If a worker is not available or doesn't support the required proof type, it will be removed from the set of workers that the proxy can use to send requests.

The status check returns:
- Whether the worker is ready to process requests
- The type of proofs the worker supports (transaction, batch, or block proofs)
- The version of the worker

### Proxy Status Endpoint

The proxy service exposes a status endpoint that provides information about the current state of the proxy and its workers. This endpoint can be accessed at `http://<proxy_host>:<status_port>/status`.

The status endpoint returns a JSON response with the following information:
- `version`: The version of the proxy
- `supported_proof_type`: The types of proof that the proxy supports
- `workers`: A list of workers with their status

Example response:
```json
{
  "version": "0.8.0",
  "prover_type": "Transaction",
  "workers": [
    {
      "address": "0.0.0.0:50051",
      "version": "0.8.0",
      "status": {
        "Unhealthy": {
          "failed_attempts": 3,
          "reason": "Unsupported prover type: Batch"
        }
      }
    },
    {
      "address": "0.0.0.0:50052",
      "version": "0.8.0",
      "status": "Healthy"
    },
  ]
}
```

The status port can be configured using the `MPS_STATUS_PORT` environment variable or the `--status-port` command-line argument when starting the proxy.

## Logging and Tracing

The service uses the [`tracing`](https://docs.rs/tracing/latest/tracing/) crate for both logging and distributed tracing, providing structured, high-performance logs and trace data.

By default, logs are written to `stdout` and the default logging level is `info`. This can be changed via the `RUST_LOG` environment variable. For example:

```
export RUST_LOG=debug
```

For tracing, we use OpenTelemetry protocol. By default, traces are exported to the endpoint specified by `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable. To consume and visualize these traces we can use Jaeger or any other OpenTelemetry compatible consumer.

The simplest way to install Jaeger is by using a [Docker](https://www.docker.com/) container. To do so, run:

```bash
docker run -d -p4317:4317 -p16686:16686 jaegertracing/all-in-one:latest
```

Then access the Jaeger UI at `http://localhost:16686/`.

If Docker is not an option, Jaeger can also be set up directly on your machine or hosted in the cloud. See the [Jaeger documentation](https://www.jaegertracing.io/docs/) for alternative installation methods.

## Metrics

The proxy includes a service that can optionally expose metrics to be consumed by [Prometheus](https://prometheus.io/docs/introduction/overview/). This service is enabled by specifying a metrics port.

### Enabling Prometheus Metrics

To enable Prometheus metrics, simply specify a port on which to expose the metrics. This can be done via environment variables or command-line arguments.

#### Using Environment Variables

Set the following environment variable:

```bash
export MPS_METRICS_PORT=6192  # Set to enable metrics on port 6192
```

To disable metrics, simply don't set the MPS_METRICS_PORT environment variable.

#### Using Command-Line Arguments

Specify a metrics port using the `--metrics-port` flag when starting the proxy:

```bash
miden-proving-service start-proxy --metrics-port 6192 [worker1] [worker2] ... [workerN]
```

If you don't specify a metrics port, metrics will be disabled.

When enabled, the Prometheus metrics will be available at `http://0.0.0.0:<metrics_port>` (e.g., `http://0.0.0.0:6192`).

The metrics architecture works by having the proxy expose metrics at an endpoint (`/metrics`) in a format Prometheus can read. Prometheus periodically scrapes this endpoint, adds timestamps to the metrics, and stores them in its time-series database. Then, we can use tools like Grafana to query Prometheus and visualize these metrics in configurable dashboards.

The simplest way to install Prometheus and Grafana is by using Docker containers. To do so, run:

```bash
docker run \
    -d \
    -p 9090:9090 \
    -v /path/to/prometheus.yml:/etc/prometheus/prometheus.yml \
    prom/prometheus

docker run -d -p 3000:3000 --name grafana grafana/grafana-enterprise:latest
```

In case that Docker is not an option, Prometheus and Grafana can also be set up directly on your machine or hosted in the cloud. See the [Prometheus documentation](https://prometheus.io/docs/prometheus/latest/getting_started/) and [Grafana documentation](https://grafana.com/docs/grafana/latest/setup-grafana/) for alternative installation methods.

A prometheus configuration file is provided in this repository, you will need to modify the `scrape_configs` section to include the URL of the proxy service (e.g., `http://0.0.0.0:6192`).

Then, to add the new Prometheus collector as a datasource for Grafana, you can [follow this tutorial](https://grafana.com/docs/grafana-cloud/connect-externally-hosted/existing-datasource/). A Grafana dashboard under the name `proxy_grafana_dashboard.json` is provided, see this [link](https://grafana.com/docs/grafana/latest/dashboards/build-dashboards/import-dashboards/) to import it. Otherwise, you can [create your own dashboard](https://grafana.com/docs/grafana/latest/getting-started/build-first-dashboard/) using the metrics provided by the proxy and export it by following this [link](https://grafana.com/docs/grafana/latest/dashboards/share-dashboards-panels/#export-a-dashboard-as-json).

## Features

Description of this crate's feature:

| Features     | Description                                                                                                 |
| ------------ | ------------------------------------------------------------------------------------------------------------|
| `concurrent` | Enables concurrent code to speed up runtime execution.                                                      |

## License

This project is [MIT licensed](../../LICENSE).

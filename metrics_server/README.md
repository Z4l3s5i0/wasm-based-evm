# metrics_server

Standalone metrics collection backend for Ethereum-compatible nodes.

## Run

```shell
cargo run -p metrics_server -- run --config metrics_server/config/example.toml
```

## Check config

```shell
cargo run -p metrics_server -- check-config --config metrics_server/config/example.toml
```

## Endpoints

- `GET /health`: Health check
- `GET /metrics`: Prometheus metrics
- `GET /api/nodes`: List configured nodes
- `GET /api/experiments`: List historical experiments
- `GET /api/metrics/latest`: Get latest metrics for all nodes
- `GET /api/metrics/range`: Query metric historical range

## Architecture

`metrics_server` is a standalone binary that interacts with Ethereum nodes only through:
- Ethereum JSON-RPC over HTTP
- Prometheus-compatible `/metrics` endpoints

It does not depend on internal node implementation crates.

## Storage

Supports `memory` and `redb` (embedded key-value store) storage backends.
Data is stored in `metrics_server/data/metrics.redb` by default.

# metrics_server

Standalone metrics collection backend for Ethereum-compatible nodes.

## Run with Docker

```shell
cd metrics_server
docker compose up -d
```

The server will be available at `http://localhost:9100`.

## Run locally (Native)

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
- `POST /api/nodes/register`: Dynamic node registration
- `GET /api/bootstrap/nodes`: Get healthy bootstrap peers
- `GET /api/experiments`: List historical experiments
- `GET /api/metrics/latest`: Get latest metrics for all nodes
- `GET /api/metrics/range`: Query metric historical range

## Bootstrap Registry

The `metrics_server` acts as a control-plane bootstrap registry. Nodes can register themselves:

```shell
curl -X POST http://127.0.0.1:9100/api/nodes/register \
  -H "Authorization: Bearer dev-token-123" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "node-3",
    "network": "devnet",
    "client": "wasix_eth",
    "rpc_url": "http://127.0.0.1:28545",
    "p2p_addr": "127.0.0.1:29002",
    "enode": "enode://pubkey@127.0.0.1:29002"
  }'
```

And retrieve healthy peers:

```shell
curl "http://127.0.0.1:9100/api/bootstrap/nodes?network=devnet&limit=16"
```

## Architecture

`metrics_server` is a standalone binary that interacts with Ethereum nodes only through:
- Ethereum JSON-RPC over HTTP
- Prometheus-compatible `/metrics` endpoints

It does not depend on internal node implementation crates.

## Storage

Supports `memory` and `redb` (embedded key-value store) storage backends.
Data is stored in `metrics_server/data/metrics.redb` by default.

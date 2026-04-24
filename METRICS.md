### Prometheus & Grafana Monitoring Plan

This document defines the metrics and dashboard structure for monitoring the Wasix EVM node.

---

### Phase 1: Infrastructure Setup

The monitoring stack consists of:
1.  **Prometheus Server:** Configured to scrape the EVM node (default `:9055/metrics`) every 1s for high-resolution benchmarking.
2.  **Node Exporter:** Captures host-level metrics (CPU, Memory, Disk, Network) on `:9100`.
3.  **Grafana:** Connected to Prometheus to render the dashboards.

---

### Phase 2: Metrics Reference

| Category | Metric Name | Type | Labels | Description |
| :--- | :--- | :--- | :--- | :--- |
| **Top Level** | `sync_status` | Gauge | - | 1: Synced, 0: Syncing, -1: Stalled |
| | `current_head_block` | Gauge | - | Local head block height |
| | `network_head_block` | Gauge | - | Highest known network block |
| | `connected_peers` | Gauge | - | Number of active peers |
| | `block_import_rate` | Gauge | - | Blocks/sec |
| | `node_uptime_seconds_total` | Counter | - | Total node uptime |
| **Chain** | `sync_gap` | Gauge | - | Δ blocks between network and local |
| | `processed_blocks_total` | Counter | - | Total blocks processed |
| | `block_processing_seconds` | Histogram | - | Time to process a block |
| | `reorgs_total` | Counter | - | Total chain reorgs |
| **P2P** | `peers_count_total` | Gauge | - | Total peer count |
| | `peer_churn_total` | Counter | - | Peer connection changes |
| | `gossip_messages_total` | Counter | `topic` | Gossip messages by topic |
| | `network_io_bytes_total` | Counter | `direction` | Inbound/Outbound bytes |
| **Execution**| `transactions_total` | Counter | - | Total transactions processed |
| | `mempool_size` | Gauge | - | Current mempool depth |
| | `gas_used_per_block` | Gauge | - | Gas used in last block |
| | `tx_execution_seconds` | Histogram | `executor` | Execution time (EVM vs WASM) |
| **RPC** | `rpc_requests_total` | Counter | `method`, `status` | Total requests and status |
| | `rpc_request_duration_seconds`| Histogram | `method` | Latency per method |
| **Storage** | `db_size_bytes` | Gauge | - | Database size on disk |
| | `cache_hits_total` | Counter | - | Storage cache hits |
| | `cache_misses_total` | Counter | - | Storage cache misses |
| **Benchmark**| `execution_time_vs_workload` | Histogram | `executor`, `workload_type` | Latency vs complexity |
| | `cpu_cycles_total` | Counter | `executor` | CPU cycles consumed |
| | `instruction_count_total` | Counter | `executor` | Instructions executed |
| | `execution_latency_cdf` | Histogram | `executor` | Cumulative Latency Distribution |
| | `wasm_compilation_seconds_total`| Counter | `executor` | Initialization overhead |

---

### Phase 3: Grafana Dashboard Layout

The monitoring setup includes two dashboards:

#### 1. Wasix EVM Multi-Node Overview (`grafana-overview-dashboard.json`)
Aggregates data from all nodes to provide a network-wide health check.
*   **Nodes Health Status:** Table showing sync status for all nodes.
*   **Block Height Comparison:** Line chart comparing head block heights.
*   **Peer Count Comparison:** Line chart comparing connected peers across nodes.
*   **Total Network Throughput:** Aggregated TPS across the entire network.

#### 2. Wasix EVM Performance & Health (`grafana-dashboard.json`)
Detailed metrics for a single node, selectable via a `node` dropdown.

#### 3. Wasix EVM Scientific Research: Native vs WASM (`grafana-research-dashboard.json`)
Specialized dashboard for comparative performance analysis.
*   **Execution Profiling:** Average execution time and instruction density (instructions per gas).
*   **Latency & Determinism:** Cumulative Distribution Function (CDF) of latency and WASM initialization overhead.
*   **Resource Footprint:** Resident memory usage and CPU cycle efficiency comparison.

Organized into the following rows:

#### 🧭 1. Top Level (The "Am I OK?" Row)
*   **Sync Status:** Stat panel showing Synced/Syncing/Stalled based on `sync_status`.
*   **Sync Gap:** `network_head_block - current_head_block`.
*   **Peers:** `connected_peers`.
*   **Import Rate:** `irate(processed_blocks_total[1m])`.

#### ⛓️ 2. Chain Sync & Block Processing
*   **Head Comparison:** Time series of local vs network head.
*   **Processing Time:** p95/p99 of `block_processing_seconds`.

#### 🌐 3. P2P Networking
*   **Peer Churn:** `rate(peer_churn_total[5m])`.
*   **Traffic:** `rate(network_io_bytes_total[5m])` by direction.

#### ⚙️ 4. System Resources (via Node Exporter)
*   **CPU Usage:** `node_cpu_seconds_total`.
*   **Disk Latency:** `node_disk_read_time_seconds_total`.

#### 🔁 5. Execution & Mempool
*   **Throughput:** `rate(transactions_total[1m])`.
*   **EVM vs WASM:** Comparison of `tx_execution_seconds`.

#### 📡 6. RPC Performance
*   **Error Rate:** `%` of `rpc_requests_total{status="error"}`.
*   **Slow Methods:** Top methods by `rpc_request_duration_seconds`.

#### 💾 7. State & DB Health
*   **Cache Hit Ratio:** `rate(cache_hits_total[5m]) / (rate(cache_hits_total[5m]) + rate(cache_misses_total[5m]))`.
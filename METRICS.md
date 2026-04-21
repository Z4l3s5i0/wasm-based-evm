### Refactored Monitoring Plan: Prometheus & Grafana Integration

This refactored plan transitions from a custom React-based dashboard to a standard industry-leading stack: **Prometheus** for metrics collection/storage and **Grafana** for visualization. This approach provides professional-grade alerting, historical data analysis, and highly performant dashboards.

---

### Phase 1: Infrastructure Setup

Instead of building a frontend dashboard from scratch, we will deploy a standard monitoring sidecar.

1.  **Prometheus Server:** Configured to scrape the EVM node at regular intervals (e.g., every 5s).
2.  **Node Exporter:** Deployed on the host machine to capture OS-level metrics (CPU, Disk, Network).
3.  **Grafana:** Connected to Prometheus as a data source to render the dashboards defined in the requirements.
4.  **Prometheus Alertmanager:** Configured to route alerts (Discord, Slack, Email) based on threshold breaches defined in Phase 3.

---

### Phase 2: Backend Instrumentation (Rust)

The `wasix-based-evm` backend must be updated to export metrics in Prometheus format.

#### 1. Integrate `prometheus` Crate
Add `prometheus` and `lazy_static` to `Cargo.toml`. Create a centralized `src/misc/metrics.rs` to register global metrics:

*   **Counters:** `processed_blocks_total`, `transactions_total`, `rpc_errors_total`, `reorgs_total`.
*   **Gauges:** `connected_peers`, `mempool_size`, `current_head_block`, `sync_gap`, `db_size_bytes`.
*   **Histograms:** `block_processing_seconds`, `rpc_request_duration_seconds`, `tx_execution_seconds`.

#### 2. Instrumentation Points
*   **Sync:** In `SyncController::sync_step`, record `current_head_block` and `block_processing_seconds`.
*   **P2P:** In `PeerManager`, update `connected_peers` gauge during `hello` and cleanup.
*   **Mempool:** In `Mempool::add_transaction`, increment `transactions_total` and update `mempool_size`.
*   **RPC:** Add a tower middleware to `RpcServerFacade` to automatically record `rpc_request_duration_seconds` and `rpc_errors_total`.

#### 3. Metrics Endpoint
Add a new HTTP endpoint (e.g., `:9090/metrics`) using `axum` that serves the Prometheus registry output.

---

### Phase 3: Grafana Dashboard Layout

The dashboard will be organized into rows as per the requirements, using specific Prometheus queries (PromQL).

#### 🧭 Row 1: Top Level (The "Am I OK?" Row)
*   **Sync Status:** `abs(head_block - network_head) < 2 ? "Synced" : "Syncing"` (Stat Panel).
*   **Sync Gap:** `network_head - current_head_block` (Sparkline).
*   **Peers:** `connected_peers` (Stat Panel).
*   **Import Rate:** `rate(processed_blocks_total[1m])` (Stat Panel).
*   **Uptime:** `process_uptime_seconds` (Stat Panel).

#### ⛓️ Row 2: Chain Sync & Block Processing
*   **Block Processing Latency:** `histogram_quantile(0.99, sum by (le) (rate(block_processing_seconds_bucket[5m])))` (Line Chart).
*   **Reorgs:** `increase(reorgs_total[1h])` (Bar Chart).

#### 🌐 Row 3: P2P Networking
*   **Peer Distribution:** `connected_peers{type="inbound"}` vs `outbound` (Stacked Area).
*   **Network Bandwidth:** `rate(node_network_receive_bytes_total[1m])` (Line Chart).

#### ⚙️ Row 4: System Resources (via Node Exporter)
*   **CPU/RAM:** `node_cpu_seconds_total` and `node_memory_MemTotal_bytes`.
*   **Disk I/O:** `rate(node_disk_read_time_seconds_total[1m])` (Critical for identifying sync bottlenecks).

#### 🔁 Row 5: Execution & Mempool
*   **Mempool Depth:** `mempool_size` (Gauge + Trend).
*   **Gas Usage:** `gas_used_per_block` (Heatmap to show block density).

#### 📡 Row 6: RPC Performance
*   **Method Latency:** `sum by (method) (rate(rpc_request_duration_seconds_sum[5m]) / rate(rpc_request_duration_seconds_count[5m]))` (Table or Bar Chart).

#### 💾 Row 7: State & DB Health
*   **Cache Hit Rate:** `cache_hits / (cache_hits + cache_misses)` (Gauge).
*   **Storage Growth:** `derivative(db_size_bytes[1d])` (Projected disk exhaustion).

---

### Phase 4: Alerting Logic

Prometheus Alerting Rules will be implemented for:
*   **Sync Stalled:** `rate(current_head_block[10m]) == 0`.
*   **High RPC Errors:** `rate(rpc_errors_total[5m]) / rate(rpc_requests_total[5m]) > 0.05`.
*   **Peer Drop:** `connected_peers < 3`.
*   **Disk Critical:** `node_filesystem_avail_bytes < 10^10` (less than 10GB remaining).

### Implementation Mapping

| Category | Source | Prometheus Metric Name (Example) |
| :--- | :--- | :--- |
| **System** | Node Exporter | `node_cpu_seconds_total`, `node_disk_io_now` |
| **Networking** | `PeerManager` | `evm_p2p_peers_count`, `evm_p2p_message_rate` |
| **Chain** | `SyncController`| `evm_chain_head_height`, `evm_chain_reorg_count` |
| **EVM** | `Executor` | `evm_execution_gas_used`, `evm_execution_tx_throughput` |
| **Mempool** | `Mempool` | `evm_mempool_pending_txs`, `evm_mempool_queued_txs` |
| **RPC** | `RpcServer` | `evm_rpc_latency_seconds`, `evm_rpc_error_count` |
| **Storage** | `InMemoryStorage`| `evm_storage_db_size_bytes`, `evm_storage_cache_hit_ratio` |
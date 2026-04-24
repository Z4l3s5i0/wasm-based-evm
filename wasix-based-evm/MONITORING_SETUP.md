# Monitoring Setup for Wasix EVM

This guide describes how to deploy the monitoring stack (Prometheus + Grafana) to visualize the metrics exported by the Wasix EVM node.

## 1. Prerequisites
- [Prometheus](https://prometheus.io/download/) installed and running.
- [Grafana](https://grafana.com/grafana/download) installed and running.
- (Optional) [Node Exporter](https://prometheus.io/download/#node_exporter) for system-level metrics.

## 2. Configure Prometheus
Copy the provided `prometheus.yml` to your Prometheus configuration directory or start Prometheus with:
```bash
prometheus --config.file=prometheus.yml
```

### 2.1. Multi-Node Configuration
To monitor multiple nodes, add them to the `static_configs` in `prometheus.yml`. Each node should have a unique `node` label:
```yaml
    static_configs:
      - targets: ['localhost:9055']
        labels:
          node: 'node-1'
      - targets: ['localhost:9056']
        labels:
          node: 'node-2'
```

## 3. Configure Grafana
1. Open Grafana (usually http://localhost:3000).
2. Go to **Connections** > **Data Sources**.
3. Add a new **Prometheus** data source pointing to your Prometheus server (e.g., `http://localhost:9090`).
4. Go to **Dashboards** > **New** > **Import**.
5. Import the dashboards:
   - `grafana-overview-dashboard.json`: Network-wide health and comparison metrics.
   - `grafana-dashboard.json`: Detailed metrics for a specific node (selectable via the `node` dropdown).
   - `grafana-research-dashboard.json`: Scientific research dashboard comparing Native vs WASM performance.
6. Select the Prometheus data source for each dashboard.

## 4. Scientific Research Mode
To run nodes for comparative research between Native and WASM execution:

1.  **Start the Native Node:**
    ```bash
    ./wasix-based-evm --executor native --metrics-port 9055
    ```
2.  **Start the WASM Node:**
    ```bash
    ./wasix-based-evm --executor wasm --metrics-port 9056
    ```
3.  **High-Frequency Benchmarking:**
    Add the `--research-mode` flag to capture granular per-transaction execution data.

## 5. Metrics Exported by Wasix EVM
The node exports metrics on `http://localhost:9090/metrics`.
Key metrics included in the dashboard:
- `current_head_block`: Local head height.
- `network_head_block`: Highest block seen in the network.
- `sync_gap`: Difference between network head and local head.
- `connected_peers`: Number of active P2P connections.
- `block_processing_seconds`: Histogram of block import times.
- `tx_execution_seconds`: Histogram of transaction execution times.
- `cache_hits_total` / `cache_misses_total`: Storage cache performance.
- `gas_used_per_block`: Gas consumption per block.
- `mempool_size`: Number of transactions in the mempool.

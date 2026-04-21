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

## 3. Configure Grafana
1. Open Grafana (usually http://localhost:3000).
2. Go to **Connections** > **Data Sources**.
3. Add a new **Prometheus** data source pointing to your Prometheus server (e.g., `http://localhost:9090`).
4. Go to **Dashboards** > **New** > **Import**.
5. Upload the `grafana-dashboard.json` file or paste its content.
6. Select the Prometheus data source you created.

## 4. Metrics Exported by Wasix EVM
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

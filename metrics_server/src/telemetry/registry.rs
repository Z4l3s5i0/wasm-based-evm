use prometheus::{Encoder, IntCounterVec, GaugeVec, Registry, TextEncoder};
use anyhow::Result;

#[derive(Clone)]
pub struct TelemetryRegistry {
    pub registry: Registry,
    pub collection_ticks: IntCounterVec,
    pub collection_errors: IntCounterVec,
    pub rpc_requests: IntCounterVec,
    pub rpc_errors: IntCounterVec,
    pub rpc_latency: GaugeVec,
    pub ethereum_rpc_up: GaugeVec,
    pub ethereum_latest_block: GaugeVec,
    pub ethereum_chain_id: GaugeVec,
    pub ethereum_peer_count: GaugeVec,
    pub ethereum_gas_price: GaugeVec,
    pub ethereum_syncing: GaugeVec,
}

impl TelemetryRegistry {
    pub fn new() -> Result<Self> {
        let registry = Registry::new();
        
        let collection_ticks = IntCounterVec::new(
            prometheus::opts!("metrics_server_collection_ticks_total", "Total number of collection ticks"),
            &[]
        )?;
        let collection_errors = IntCounterVec::new(
            prometheus::opts!("metrics_server_collection_errors_total", "Total number of collection errors"),
            &["node_id", "source"]
        )?;
        let rpc_requests = IntCounterVec::new(
            prometheus::opts!("metrics_server_rpc_requests_total", "Total number of RPC requests"),
            &["node_id", "method"]
        )?;
        let rpc_errors = IntCounterVec::new(
            prometheus::opts!("metrics_server_rpc_errors_total", "Total number of RPC errors"),
            &["node_id", "method"]
        )?;
        let rpc_latency = GaugeVec::new(
            prometheus::opts!("metrics_server_rpc_latency_ms", "RPC latency in milliseconds"),
            &["node_id", "method"]
        )?;
        let ethereum_rpc_up = GaugeVec::new(
            prometheus::opts!("ethereum_rpc_up", "Ethereum RPC up status"),
            &["node_id", "network", "client"]
        )?;
        let ethereum_latest_block = GaugeVec::new(
            prometheus::opts!("ethereum_latest_block", "Latest block number"),
            &["node_id", "network", "client"]
        )?;
        let ethereum_chain_id = GaugeVec::new(
            prometheus::opts!("ethereum_chain_id", "Ethereum chain ID"),
            &["node_id", "network", "client"]
        )?;
        let ethereum_peer_count = GaugeVec::new(
            prometheus::opts!("ethereum_peer_count", "Ethereum peer count"),
            &["node_id", "network", "client"]
        )?;
        let ethereum_gas_price = GaugeVec::new(
            prometheus::opts!("ethereum_gas_price", "Ethereum gas price"),
            &["node_id", "network", "client"]
        )?;
        let ethereum_syncing = GaugeVec::new(
            prometheus::opts!("ethereum_syncing", "Ethereum syncing status"),
            &["node_id", "network", "client"]
        )?;

        registry.register(Box::new(collection_ticks.clone()))?;
        registry.register(Box::new(collection_errors.clone()))?;
        registry.register(Box::new(rpc_requests.clone()))?;
        registry.register(Box::new(rpc_errors.clone()))?;
        registry.register(Box::new(rpc_latency.clone()))?;
        registry.register(Box::new(ethereum_rpc_up.clone()))?;
        registry.register(Box::new(ethereum_latest_block.clone()))?;
        registry.register(Box::new(ethereum_chain_id.clone()))?;
        registry.register(Box::new(ethereum_peer_count.clone()))?;
        registry.register(Box::new(ethereum_gas_price.clone()))?;
        registry.register(Box::new(ethereum_syncing.clone()))?;

        Ok(Self {
            registry,
            collection_ticks,
            collection_errors,
            rpc_requests,
            rpc_errors,
            rpc_latency,
            ethereum_rpc_up,
            ethereum_latest_block,
            ethereum_chain_id,
            ethereum_peer_count,
            ethereum_gas_price,
            ethereum_syncing,
        })
    }

    pub fn gather_text(&self) -> Result<String> {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }
}

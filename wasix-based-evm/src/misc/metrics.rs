use prometheus::{
    register_counter, register_counter_vec, register_gauge, register_gauge_vec, register_histogram,
    register_histogram_vec, Counter, CounterVec, Gauge, GaugeVec, Histogram, HistogramVec,
};
use lazy_static::lazy_static;

lazy_static! {
    pub static ref SYNC_STATUS: Gauge = register_gauge!("sync_status", "Sync status (1: synced, 0: syncing, -1: stalled)").unwrap();
    pub static ref CURRENT_HEAD_BLOCK: Gauge = register_gauge!("current_head_block", "Current head block height").unwrap();
    pub static ref NETWORK_HEAD: Gauge = register_gauge!("network_head_block", "Highest block height known in the network").unwrap();
    pub static ref CONNECTED_PEERS: Gauge = register_gauge!("connected_peers", "Number of currently connected peers").unwrap();
    pub static ref BLOCK_IMPORT_RATE: Gauge = register_gauge!("block_import_rate", "Blocks imported per second").unwrap();
    pub static ref NODE_UPTIME: Counter = register_counter!("node_uptime_seconds_total", "Total time the node has been running").unwrap();
}

pub fn init_metrics() {
    let _ = *SYNC_STATUS;
    let _ = *CURRENT_HEAD_BLOCK;
    let _ = *NETWORK_HEAD;
    let _ = *CONNECTED_PEERS;
    let _ = *BLOCK_IMPORT_RATE;
    let _ = *NODE_UPTIME;
}

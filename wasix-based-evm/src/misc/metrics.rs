use prometheus::{
    register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram,
};
use lazy_static::lazy_static;

lazy_static! {
    // Counters
    pub static ref PROCESSED_BLOCKS: Counter = register_counter!("processed_blocks_total", "Total number of processed blocks").unwrap();
    pub static ref TRANSACTIONS_TOTAL: Counter = register_counter!("transactions_total", "Total number of processed transactions").unwrap();
    pub static ref RPC_ERRORS_TOTAL: Counter = register_counter!("rpc_errors_total", "Total number of RPC errors").unwrap();
    pub static ref REORGS_TOTAL: Counter = register_counter!("reorgs_total", "Total number of chain reorgs").unwrap();
    pub static ref CACHE_HITS: Counter = register_counter!("cache_hits_total", "Total number of storage cache hits").unwrap();
    pub static ref CACHE_MISSES: Counter = register_counter!("cache_misses_total", "Total number of storage cache misses").unwrap();

    // Gauges
    pub static ref CONNECTED_PEERS: Gauge = register_gauge!("connected_peers", "Number of currently connected peers").unwrap();
    pub static ref MEMPOOL_SIZE: Gauge = register_gauge!("mempool_size", "Current number of transactions in mempool").unwrap();
    pub static ref CURRENT_HEAD_BLOCK: Gauge = register_gauge!("current_head_block", "Current head block height").unwrap();
    pub static ref NETWORK_HEAD: Gauge = register_gauge!("network_head_block", "Highest block height known in the network").unwrap();
    pub static ref SYNC_GAP: Gauge = register_gauge!("sync_gap", "Gap between network head and local head").unwrap();
    pub static ref DB_SIZE_BYTES: Gauge = register_gauge!("db_size_bytes", "Size of the database in bytes").unwrap();
    pub static ref GAS_USED_PER_BLOCK: Gauge = register_gauge!("gas_used_per_block", "Gas used in the last processed block").unwrap();

    // Histograms
    pub static ref BLOCK_PROCESSING_SECONDS: Histogram = register_histogram!("block_processing_seconds", "Time taken to process a block").unwrap();
    pub static ref RPC_REQUEST_DURATION_SECONDS: Histogram = register_histogram!("rpc_request_duration_seconds", "Duration of RPC requests").unwrap();
    pub static ref TX_EXECUTION_SECONDS: Histogram = register_histogram!("tx_execution_seconds", "Time taken to execute a transaction").unwrap();
}

pub fn init_metrics() {
    // Access each static to ensure it is initialized and registered
    let _ = *PROCESSED_BLOCKS;
    let _ = *TRANSACTIONS_TOTAL;
    let _ = *RPC_ERRORS_TOTAL;
    let _ = *REORGS_TOTAL;
    let _ = *CACHE_HITS;
    let _ = *CACHE_MISSES;
    let _ = *CONNECTED_PEERS;
    let _ = *MEMPOOL_SIZE;
    let _ = *CURRENT_HEAD_BLOCK;
    let _ = *NETWORK_HEAD;
    let _ = *SYNC_GAP;
    let _ = *DB_SIZE_BYTES;
    let _ = *GAS_USED_PER_BLOCK;
    let _ = *BLOCK_PROCESSING_SECONDS;
    let _ = *RPC_REQUEST_DURATION_SECONDS;
    let _ = *TX_EXECUTION_SECONDS;
}

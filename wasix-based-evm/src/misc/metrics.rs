use prometheus::{
    register_counter, register_counter_vec, register_gauge, register_histogram, register_histogram_vec,
    Counter, CounterVec, Gauge, Histogram, HistogramVec,
};
use lazy_static::lazy_static;

lazy_static! {
    // --- 🧭 Top row: "Am I OK?" ---
    pub static ref SYNC_STATUS: Gauge = register_gauge!("sync_status", "Sync status (1: synced, 0: syncing, -1: stalled)").unwrap();
    pub static ref CURRENT_HEAD_BLOCK: Gauge = register_gauge!("current_head_block", "Current head block height").unwrap();
    pub static ref NETWORK_HEAD: Gauge = register_gauge!("network_head_block", "Highest block height known in the network").unwrap();
    pub static ref CONNECTED_PEERS: Gauge = register_gauge!("connected_peers", "Number of currently connected peers").unwrap();
    pub static ref BLOCK_IMPORT_RATE: Gauge = register_gauge!("block_import_rate", "Blocks imported per second").unwrap();
    pub static ref NODE_UPTIME: Counter = register_counter!("node_uptime_seconds_total", "Total time the node has been running").unwrap();

    // --- ⛓️ Chain sync & block processing ---
    pub static ref SYNC_GAP: Gauge = register_gauge!("sync_gap", "Gap between network head and local head").unwrap();
    pub static ref PROCESSED_BLOCKS: Counter = register_counter!("processed_blocks_total", "Total number of processed blocks").unwrap();
    pub static ref BLOCK_PROCESSING_SECONDS: Histogram = register_histogram!("block_processing_seconds", "Time taken to process a block").unwrap();
    pub static ref REORGS_TOTAL: Counter = register_counter!("reorgs_total", "Total number of chain reorgs").unwrap();

    // --- 🌐 P2P networking ---
    pub static ref PEERS_COUNT: Gauge = register_gauge!("peers_count_total", "Total number of peers").unwrap();
    pub static ref PEER_CHURN: Counter = register_counter!("peer_churn_total", "Total number of peer connection changes").unwrap();
    pub static ref GOSSIP_MESSAGES_TOTAL: CounterVec = register_counter_vec!("gossip_messages_total", "Total number of gossip messages", &["topic"]).unwrap();
    pub static ref NETWORK_IO_BYTES: CounterVec = register_counter_vec!("network_io_bytes_total", "Total network I/O in bytes", &["direction"]).unwrap();

    // --- 🔁 Execution / EVM / mempool ---
    pub static ref TRANSACTIONS_TOTAL: Counter = register_counter!("transactions_total", "Total number of processed transactions").unwrap();
    pub static ref MEMPOOL_SIZE: Gauge = register_gauge!("mempool_size", "Current number of transactions in mempool").unwrap();
    pub static ref GAS_USED_PER_BLOCK: Gauge = register_gauge!("gas_used_per_block", "Gas used in the last processed block").unwrap();
    pub static ref TX_EXECUTION_SECONDS: HistogramVec = register_histogram_vec!("tx_execution_seconds", "Time taken to execute a transaction", &["executor"]).unwrap();

    // --- 📡 RPC / API performance ---
    pub static ref RPC_REQUESTS_TOTAL: CounterVec = register_counter_vec!("rpc_requests_total", "Total number of RPC requests", &["method", "status"]).unwrap();
    pub static ref RPC_REQUEST_DURATION_SECONDS: HistogramVec = register_histogram_vec!("rpc_request_duration_seconds", "Duration of RPC requests", &["method"]).unwrap();

    // --- 💾 State & database health ---
    pub static ref DB_SIZE_BYTES: Gauge = register_gauge!("db_size_bytes", "Size of the database in bytes").unwrap();
    pub static ref CACHE_HITS: Counter = register_counter!("cache_hits_total", "Total number of storage cache hits").unwrap();
    pub static ref CACHE_MISSES: Counter = register_counter!("cache_misses_total", "Total number of storage cache misses").unwrap();

    // --- 📊 Benchmarking (EVM vs WASM) ---
    pub static ref EXECUTION_TIME_VS_WORKLOAD: HistogramVec = register_histogram_vec!("execution_time_vs_workload", "Execution time vs workload size", &["executor", "workload_type"]).unwrap();
    pub static ref CPU_CYCLES_TOTAL: CounterVec = register_counter_vec!("cpu_cycles_total", "Total CPU cycles consumed", &["executor"]).unwrap();
    pub static ref INSTRUCTION_COUNT_TOTAL: CounterVec = register_counter_vec!("instruction_count_total", "Total instructions executed", &["executor"]).unwrap();

    // --- 🔬 Research specific ---
    pub static ref EXECUTION_LATENCY_CDF: HistogramVec = register_histogram_vec!(
        "execution_latency_cdf",
        "Cumulative distribution of execution latency",
        &["executor"],
        vec![0.00001, 0.00005, 0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 2.0, 5.0]
    ).unwrap();
    pub static ref WASM_COMPILATION_SECONDS: CounterVec = register_counter_vec!("wasm_compilation_seconds_total", "Total time spent on WASM compilation/initialization", &["executor"]).unwrap();
}

pub fn init_metrics() {
    let _ = *SYNC_STATUS;
    let _ = *CURRENT_HEAD_BLOCK;
    let _ = *NETWORK_HEAD;
    let _ = *CONNECTED_PEERS;
    let _ = *BLOCK_IMPORT_RATE;
    let _ = *NODE_UPTIME;
    let _ = *SYNC_GAP;
    let _ = *PROCESSED_BLOCKS;
    let _ = *BLOCK_PROCESSING_SECONDS;
    let _ = *REORGS_TOTAL;
    let _ = *PEERS_COUNT;
    let _ = *PEER_CHURN;
    let _ = *GOSSIP_MESSAGES_TOTAL;
    let _ = *NETWORK_IO_BYTES;
    let _ = *TRANSACTIONS_TOTAL;
    let _ = *MEMPOOL_SIZE;
    let _ = *GAS_USED_PER_BLOCK;
    let _ = *TX_EXECUTION_SECONDS;
    let _ = *RPC_REQUESTS_TOTAL;
    let _ = *RPC_REQUEST_DURATION_SECONDS;
    let _ = *DB_SIZE_BYTES;
    let _ = *CACHE_HITS;
    let _ = *CACHE_MISSES;
    let _ = *EXECUTION_TIME_VS_WORKLOAD;
    let _ = *CPU_CYCLES_TOTAL;
    let _ = *INSTRUCTION_COUNT_TOTAL;
    let _ = *EXECUTION_LATENCY_CDF;
    let _ = *WASM_COMPILATION_SECONDS;
}

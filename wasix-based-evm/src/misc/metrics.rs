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

    // Execution Metrics
    pub static ref BLOCK_EXECUTION_TIME: Histogram = register_histogram!("block_execution_time_seconds", "Time taken to execute a block").unwrap();
    pub static ref TRANSACTION_EXECUTION_TIME: Histogram = register_histogram!("transaction_execution_time_seconds", "Time taken to execute a transaction").unwrap();
    pub static ref GAS_PROCESSED_TOTAL: Counter = register_counter!("gas_processed_total", "Total gas processed by the executor").unwrap();

    // Mempool Metrics
    pub static ref MEMPOOL_SIZE: Gauge = register_gauge!("mempool_size", "Current number of transactions in the mempool").unwrap();
    pub static ref MEMPOOL_REJECTED_TRANSACTIONS: Counter = register_counter!("mempool_rejected_transactions_total", "Total number of transactions rejected by the mempool").unwrap();

    // Network Metrics
    pub static ref GOSSIP_MESSAGES_RECEIVED: Counter = register_counter!("gossip_messages_received_total", "Total gossip messages received").unwrap();
    pub static ref P2P_MESSAGES_SENT_BYTES: Counter = register_counter!("p2p_messages_sent_bytes_total", "Total bytes sent over the p2p network").unwrap();

    // Storage Metrics
    pub static ref STORAGE_READ_LATENCY: Histogram = register_histogram!("storage_read_latency_seconds", "Latency of storage read operations").unwrap();
    pub static ref STORAGE_WRITE_LATENCY: Histogram = register_histogram!("storage_write_latency_seconds", "Latency of storage write operations").unwrap();

    // Microarchitecture Metrics
    pub static ref INSTRUCTION_COUNT_TOTAL: Counter = register_counter!("instruction_count_total", "Total number of instructions executed").unwrap();
    pub static ref CPU_CYCLES_TOTAL: Counter = register_counter!("cpu_cycles_total", "Total number of CPU cycles consumed").unwrap();

    // Consensus Metrics
    pub static ref FORK_CHOICE_UPDATED_TOTAL: Counter = register_counter!("fork_choice_updated_total", "Total number of fork choice updates").unwrap();
    pub static ref REORG_COUNT_TOTAL: Counter = register_counter!("reorg_count_total", "Total number of chain reorgs").unwrap();

    // Block Production Metrics
    pub static ref BLOCK_PRODUCTION_SUCCESS: Counter = register_counter!("block_production_success_total", "Total successful block productions").unwrap();
    pub static ref BLOCK_PRODUCTION_FAILED: Counter = register_counter!("block_production_failed_total", "Total failed block productions").unwrap();
}

pub fn init_metrics() {
    let _ = *SYNC_STATUS;
    let _ = *CURRENT_HEAD_BLOCK;
    let _ = *NETWORK_HEAD;
    let _ = *CONNECTED_PEERS;
    let _ = *BLOCK_IMPORT_RATE;
    let _ = *NODE_UPTIME;

    let _ = *BLOCK_EXECUTION_TIME;
    let _ = *TRANSACTION_EXECUTION_TIME;
    let _ = *GAS_PROCESSED_TOTAL;
    let _ = *MEMPOOL_SIZE;
    let _ = *MEMPOOL_REJECTED_TRANSACTIONS;
    let _ = *GOSSIP_MESSAGES_RECEIVED;
    let _ = *P2P_MESSAGES_SENT_BYTES;
    let _ = *STORAGE_READ_LATENCY;
    let _ = *STORAGE_WRITE_LATENCY;

    let _ = *INSTRUCTION_COUNT_TOTAL;
    let _ = *CPU_CYCLES_TOTAL;
    let _ = *FORK_CHOICE_UPDATED_TOTAL;
    let _ = *REORG_COUNT_TOTAL;
    let _ = *BLOCK_PRODUCTION_SUCCESS;
    let _ = *BLOCK_PRODUCTION_FAILED;
}

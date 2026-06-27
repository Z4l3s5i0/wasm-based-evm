use prometheus::{
    register_counter, register_gauge, register_histogram, Counter, Gauge, Histogram,
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
    pub static ref INSTRUCTIONS_PROCESSED_TOTAL: Counter = register_counter!("instructions_processed_total", "Total instructions processed by the executor").unwrap();

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

    // RPC Metrics
    pub static ref RPC_REQUESTS_TOTAL: Counter = register_counter!("rpc_requests_total", "Total number of RPC requests received").unwrap();
    pub static ref RPC_REQUEST_DURATION: Histogram = register_histogram!("rpc_request_duration_seconds", "Latency of RPC requests").unwrap();
    pub static ref RPC_ERRORS_TOTAL: Counter = register_counter!("rpc_errors_total", "Number of failed RPC requests").unwrap();

    // Advanced P2P Metrics
    pub static ref P2P_PEERS_BY_CLIENT_TYPE: Gauge = register_gauge!("p2p_peers_by_client_type", "Number of connected peers categorized by client type").unwrap();
    pub static ref P2P_DISCOVERY_NODES_FOUND: Counter = register_counter!("p2p_discovery_nodes_found_total", "Total nodes discovered via the discovery protocol").unwrap();
    pub static ref P2P_CONNECTION_ERRORS_TOTAL: Counter = register_counter!("p2p_connection_errors_total", "Frequency of connection failures").unwrap();

    // Advanced Execution & WASM Metrics
    pub static ref WASM_COMPILATION_TIME: Histogram = register_histogram!("wasm_compilation_time_seconds", "Time taken to compile EVM bytecode into WASM").unwrap();
    pub static ref WASM_MEMORY_USAGE: Gauge = register_gauge!("wasm_memory_usage_bytes", "Current memory consumption of the WASM runtime instances").unwrap();
    pub static ref STATE_DB_CACHE_HIT_RATIO: Gauge = register_gauge!("state_db_cache_hit_ratio", "Efficiency of the state database cache").unwrap();

    // Mempool Enhancements
    pub static ref MEMPOOL_PENDING_TRANSACTIONS: Gauge = register_gauge!("mempool_pending_transactions", "Number of transactions waiting to be included in a block").unwrap();
    pub static ref MEMPOOL_QUEUED_TRANSACTIONS: Gauge = register_gauge!("mempool_queued_transactions", "Number of transactions that are currently non-executable").unwrap();
    pub static ref MEMPOOL_TRANSACTION_AGE: Histogram = register_histogram!("mempool_transaction_age_seconds", "How long a transaction stays in the mempool").unwrap();

    // Chain & Consensus Health
    pub static ref CHAIN_HEAD_AGE: Gauge = register_gauge!("chain_head_age_seconds", "Time elapsed since the last block was received or produced").unwrap();
    pub static ref INVALID_BLOCKS_RECEIVED: Counter = register_counter!("invalid_blocks_received_total", "Number of blocks that failed validation").unwrap();
    pub static ref BLOCK_GAS_UTILIZATION: Gauge = register_gauge!("block_gas_utilization", "The ratio of gas used to the gas limit in recent blocks").unwrap();

    // System Resource Monitoring
    pub static ref PROCESS_CPU_SECONDS: Counter = register_counter!("process_cpu_seconds_total", "CPU time consumed by the node process").unwrap();
    pub static ref PROCESS_RESIDENT_MEMORY: Gauge = register_gauge!("process_resident_memory_bytes", "Resident set size (RSS) memory used by the process").unwrap();
    pub static ref FILE_DESCRIPTORS_OPEN: Gauge = register_gauge!("file_descriptors_open", "Number of open file descriptors").unwrap();

    // Storage Performance
    pub static ref STORAGE_DB_SIZE: Gauge = register_gauge!("storage_db_size_bytes", "Total size of the underlying database on disk").unwrap();
    pub static ref STORAGE_CACHE_SIZE: Gauge = register_gauge!("storage_cache_size_bytes", "Memory used by database caches").unwrap();

    // Granular Network Metrics
    pub static ref P2P_MESSAGES_RECEIVED: Counter = register_counter!("p2p_messages_received_total", "Total P2P messages received by type").unwrap();
    pub static ref P2P_PEERS_CONNECTED: Counter = register_counter!("p2p_peers_connected_total", "Total number of peer connections established").unwrap();
    pub static ref P2P_PEERS_DISCONNECTED: Counter = register_counter!("p2p_peers_disconnected_total", "Total number of peer disconnections").unwrap();

    // Granular Storage Metrics
    pub static ref STORAGE_OPERATIONS: Counter = register_counter!("storage_operations_total", "Total number of storage operations by table and type").unwrap();
    pub static ref STORAGE_TRIE_COMPUTATION_TIME: Histogram = register_histogram!("storage_trie_computation_time_seconds", "Time taken to compute trie roots").unwrap();

    // Granular Execution Metrics
    pub static ref EXECUTION_VALIDATION_ERRORS: Counter = register_counter!("execution_validation_errors_total", "Total number of transaction validation errors").unwrap();
    pub static ref EXECUTION_PRECOMPILE_CALLS: Counter = register_counter!("execution_precompile_calls_total", "Total number of precompile calls").unwrap();

    // Sync Enhancements
    pub static ref SYNC_TARGET_HEIGHT: Gauge = register_gauge!("sync_target_height", "The target block height the node is syncing towards").unwrap();
    pub static ref SYNC_REMAINING_BLOCKS: Gauge = register_gauge!("sync_remaining_blocks", "Number of blocks remaining to be synced").unwrap();
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

    let _ = *RPC_REQUESTS_TOTAL;
    let _ = *RPC_REQUEST_DURATION;
    let _ = *RPC_ERRORS_TOTAL;

    let _ = *P2P_PEERS_BY_CLIENT_TYPE;
    let _ = *P2P_DISCOVERY_NODES_FOUND;
    let _ = *P2P_CONNECTION_ERRORS_TOTAL;

    let _ = *WASM_COMPILATION_TIME;
    let _ = *WASM_MEMORY_USAGE;
    let _ = *STATE_DB_CACHE_HIT_RATIO;

    let _ = *MEMPOOL_PENDING_TRANSACTIONS;
    let _ = *MEMPOOL_QUEUED_TRANSACTIONS;
    let _ = *MEMPOOL_TRANSACTION_AGE;

    let _ = *CHAIN_HEAD_AGE;
    let _ = *INVALID_BLOCKS_RECEIVED;
    let _ = *BLOCK_GAS_UTILIZATION;

    let _ = *PROCESS_CPU_SECONDS;
    let _ = *PROCESS_RESIDENT_MEMORY;
    let _ = *FILE_DESCRIPTORS_OPEN;

    let _ = *STORAGE_DB_SIZE;
    let _ = *STORAGE_CACHE_SIZE;

    let _ = *P2P_MESSAGES_RECEIVED;
    let _ = *P2P_PEERS_CONNECTED;
    let _ = *P2P_PEERS_DISCONNECTED;

    let _ = *STORAGE_OPERATIONS;
    let _ = *STORAGE_TRIE_COMPUTATION_TIME;

    let _ = *EXECUTION_VALIDATION_ERRORS;
    let _ = *EXECUTION_PRECOMPILE_CALLS;

    let _ = *SYNC_TARGET_HEIGHT;
    let _ = *SYNC_REMAINING_BLOCKS;
}

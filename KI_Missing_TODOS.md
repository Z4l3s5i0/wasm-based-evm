Based on the previous analysis and the current state of the repository, here is a comprehensive implementation plan for the remaining tasks to achieve full Ethereum P2P compliance and production readiness.

### Implementation Plan

#### 1. Configuration and Service Management
The first step is to move away from hardcoded values and allow services to be configured via flags or a configuration file.

*   **Task 1.1: Expand `NetworkConfig` and Add CLI Flags**
    *   Update `src/network/mod.rs` to include more granular configuration.
    *   Implement a configuration loader in `src/main.rs` that reads from environment variables or command-line arguments.
    *   **TODO: Configuration Flags to implement:**
        *   `--p2p-port`: TCP port for RLPx/Tentacle (default: `9001`).
        *   `--discovery-port`: UDP port for Discv5 (default: `9000`).
        *   `--rpc-port`: gRPC/JSON-RPC port (default: `50051`).
        *   `--bootnodes`: Comma-separated list of ENRs for bootstrapping.
        *   `--max-peers`: Maximum number of concurrent P2P connections.
        *   `--data-dir`: Path for persistent storage (RocksDB/MDBX).

#### 2. Networking Layer Evolution (RLPx & Protocol)
Transition from a basic `tentacle` implementation to a more Ethereum-compliant stack.

*   **Task 2.1: Implement RLPx Transport & Framing**
    *   Replace or wrap `Secio` with an ECIES-encrypted handshake.
    *   Implement Snappy compression for protocol messages.
    *   Implement the `Hello` (p2p/5) message exchange before the `eth` protocol starts.
*   **Task 2.2: Complete `eth/67` Support**
    *   In `src/network/protocol.rs`, implement missing handlers for:
        *   `GetBlockHeaders` / `BlockHeaders`
        *   `GetBlockBodies` / `BlockBodies`
        *   `GetPooledTransactions` / `PooledTransactions`
    *   Transition transaction gossip to use `NewPooledTransactionHashes` (64-bit IDs).

#### 3. Synchronization Logic
Enable the client to catch up with the network without relying on manual block injection.

*   **Task 3.1: Header & Body Sync**
    *   Implement a `SyncService` that orchestrates fetching headers from multiple peers.
    *   Validate header difficulty and chain consistency.
    *   Download corresponding block bodies and execute them through the `Executor`.
*   **Task 3.2: Snap Sync (Optional but Recommended)**
    *   Implement the `snap` protocol to allow fetching the state trie directly for faster synchronization.

#### 4. Persistence Layer
Replace `InMemoryStorage` with a disk-backed solution to ensure data survives restarts.

*   **Task 4.1: Disk-Backed Key-Value Store**
    *   Integrate a WASI-compatible storage backend (e.g., a port of RocksDB or a simpler KV store if running in restricted environments).
    *   Refactor `src/storage.rs` to persist `Block`, `Transaction`, and `Receipt` data.
*   **Task 4.2: State Trie Persistence**
    *   Ensure the `alloy-trie` based state is committed to disk at the end of every block execution.

#### 5. Peer Management and Robustness
*   **Task 5.1: Active Peer Tracking**
    *   Replace TODOs in `src/network/service.rs` (e.g., `GetPeerCount`, `GetPeers`) with actual logic tracking active `tentacle` sessions.
    *   Implement peer reputation scoring (disconnecting peers that send invalid blocks or time out).
*   **Task 5.2: Official Bootnodes Integration**
    *   Populate the default configuration with official Ethereum mainnet/testnet bootnodes to allow "zero-config" discovery.

### Summary of Configuration TODOs (Flags)
The following is a suggested checklist for the configuration service implementation:

```rust
// TODO: Implement reading these from CLI/Env in main.rs
pub struct AppConfig {
    pub p2p_tcp_port: u16,      // --p2p-port
    pub discovery_udp_port: u16,// --discovery-port
    pub rpc_port: u16,          // --rpc-port
    pub bootnodes: Vec<String>, // --bootnodes
    pub data_dir: PathBuf,      // --data-dir
    pub chain: String,          // --chain (mainnet, sepolia, devnet)
    pub max_peers: usize,       // --max-peers
}
```

### Next Immediate Steps
1.  Modify `main.rs` to use `clap` or a similar crate for argument parsing.
2.  Update `NetworkService::new` in `src/network/service.rs` to utilize the new port flags.
3.  Begin implementing the `GetBlockHeaders` handler in `src/network/protocol.rs`.
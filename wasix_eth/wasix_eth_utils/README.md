# `wasix_eth_utils`

## Overview

* **Purpose:** The `wasix_eth_utils` crate provides shared utility functions, data mappers, logging infrastructure, and metrics definitions for the WASIX Ethereum project.
* **Responsibilities:**
    * **Logging:** Providing a custom macro-based logging system (`info!`, `debug!`, `warn!`, `error!`) with log level control and in-memory log buffer.
    * **Metrics:** Defining and initializing Prometheus metrics for monitoring node performance (sync, execution, network, storage).
    * **Identity:** Managing P2P node identity, including key generation, persistence, and PeerId calculation.
    * **Mappers:** Translating between internal domain types (Blocks, Transactions, Receipts) and API-specific types (RPC, Engine API).
* **High-level design:** The crate is a collection of decoupled modules. It heavily uses static globals and lazy initialization (via `once_cell` and `lazy_static`) for logging and metrics to ensure they are accessible globally without explicit dependency injection.
* **Fit:** This is a low-level utility crate depended upon by almost every other crate in the project to provide consistent logging, monitoring, and data transformation logic.

---

## Module Structure

```text
src/
├── lib.rs                  # Module declarations
├── logging.rs              # Custom logging macros and level management
├── metrics.rs              # Prometheus metrics definitions and initialization
├── identity.rs             # P2P Node Identity and keypair management
├── block_mapper.rs         # Conversion from internal Blocks to RPC types
├── transaction_mapper.rs   # Conversion from Transactions/Receipts to RPC types
└── engine_mapper.rs        # Conversion for Engine API (ExecutionPayloads)
```

---

## Public API

### `logging` Macros (`logging.rs`)
Custom logging system.

**Macros:**
* `info!(...)`: Logs informational messages.
* `debug!(...)`: Logs detailed debugging information.
* `warn!(...)`: Logs warnings.
* `error!(...)`: Logs errors (to stderr).

**Important Functions:**
* `set_log_level(level: LogLevel)`: Sets the global log filter level.
* `get_log_level() -> LogLevel`: Retrieves the current log level.

### `Identity` (`identity.rs`)
Manages the P2P node's cryptographic identity.

**Responsibilities:**
- Loading or generating the P2P private key.
- Persisting the key to disk in the data directory.
- Providing the public key in various formats (PeerId, SEC1, B512).

**Important Methods:**
* `new(data_dir, peer_name)`: Loads or creates an identity.
* `peer_id() -> String`: Returns the hex-encoded PeerId.
* `public_key_b512() -> B512`: Returns the 64-byte uncompressed public key.

### `BlockMapper` (`block_mapper.rs`)
Converts internal block types to RPC-compatible structures.

**Important Methods:**
* `to_rpc_block(block, full, chain_config, total_difficulty) -> RpcBlock`

### `TransactionMapper` (`transaction_mapper.rs`)
Converts transactions and receipts for RPC.

**Important Methods:**
* `to_rpc_transaction(...) -> RpcTransaction`
* `to_rpc_receipt(...) -> RpcTransactionReceipt`
* `parse_address(addr: &str) -> Result<Address, RpcError>`

---

## Internal Architecture

* **Major Components:**
    * **Static Global State:** Logging levels and the metrics registry are stored in global variables for easy access.
    * **In-Memory Log Buffer:** A `VecDeque` with a capacity of 1000 logs is maintained for administrative queries.
    * **Pure Mapping Logic:** Mappers are stateless structs with static methods for type conversion.
* **Synchronization Primitives:**
    * `std::sync::Mutex` is used for thread-safe access to the global log buffer.
    * `once_cell::sync::Lazy` and `lazy_static!` ensure thread-safe initialization of global registries.
* **Async Architecture:** While the utilities are mostly synchronous, the mappers are designed to be used within async contexts (e.g., RPC handlers).

---

## Dependency Graph

| Dependency | Purpose |
| ---------- | ------- |
| `wasix_eth_types` | Domain type definitions |
| `prometheus` | Metrics library |
| `chrono` | Timestamp generation for logs |
| `k256` / `secp256k1` | ECDSA signing and key management |
| `alloy-rlp` | RLP encoding for block size calculation |
| `once_cell` / `lazy_static` | Thread-safe static initialization |
| `anyhow` | Error handling |

---

## Error Handling

* **Result Usage:** Identity management and address parsing use `anyhow::Result` or `RpcResult` for error propagation.
* **Logging Strategy:** The crate provides the `error!` macro used throughout the project to report failures.
* **Custom Errors:** Identity loading uses `anyhow::Context` to provide detailed path-related error messages.

---

## Configuration

* **Log Level:** Controlled via `logging::set_log_level`.
* **Identity Persistence:** Private keys are saved as `p2p_<peer_name>.key` in the provided data directory.
* **Metrics:** All metrics are registered in the global Prometheus registry upon calling `init_metrics()`.

---

## Testing

* **Unit Tests:**
    * `transaction_mapper.rs` contains tests for receipt hash preservation.
    * `logging.rs` behavior is verified through standard project execution.
* **Mocking:** The `NoopGossip` (found in `wasix_eth_types` but used with these utils) allows for isolated testing of components.

---

## Extension Points

* **New Metrics:** Add a static ref in `metrics.rs` and update `init_metrics`.
* **New Mappers:** New domain-to-API mappings can be added to existing or new mapper files.
* **Logging Sinks:** The `add_log` function can be extended to support file-based logging or external log aggregators.

---

## Known Limitations

* **Log Buffer Size:** The in-memory log buffer is fixed at 1000 entries.
* **Unsafe Code:** Log level management uses `static mut` and `unsafe` blocks for performance, relying on minimal thread contention for level changes.
* **Receipt Indexing:** `log_index` in `to_rpc_receipt` is currently a simplified index within the transaction rather than the absolute index in the block.

---

## Cross-Crate Relationships

* **Depends on:** `wasix_eth_types`.
* **Used by:** `wasix_eth_app`, `wasix_eth_core`, `wasix_eth_p2p`, `wasix_eth_rpc`, `wasix_eth_storage`.
* **Primary Interface:** Global macros and static provider methods.

---

## Developer Notes

* **Logging Performance:** Logging involves string formatting and locking a mutex; avoid logging large amounts of data in tight loops unless at the `Debug` level.
* **Identity Safety:** The `Identity` struct provides access to the node's private key. Handle with care and ensure it is not leaked.
* **Metrics Initialization:** Ensure `init_metrics()` is called exactly once at application startup (typically in `AppBuilder`).

---

## Summary

`wasix_eth_utils` provides the essential supporting infrastructure for the WASIX Ethereum node. By centralizing logging, metrics, identity, and type mapping, it ensures that high-level components remain focused on their core logic while benefiting from consistent diagnostic and monitoring capabilities. Its low-dependency design makes it a stable foundation for the entire project.
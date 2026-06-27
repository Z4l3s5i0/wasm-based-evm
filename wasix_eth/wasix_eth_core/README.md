# `wasix_eth_core`

## Overview

* **Purpose:** The `wasix_eth_core` crate serves as the central brain of the WASIX Ethereum node. It coordinates consensus, chain management, transaction pooling, and the Engine API.
* **Responsibilities:**
    * Implementing the Ethereum Engine API (Consensus-Execution separation).
    * Managing the blockchain state, head block, and chain reorgs.
    * Maintaining a pool of pending transactions (Mempool).
    * Orchestrating block production (Payload Building) and block import.
    * Enforcing consensus rules.
    * Managing node-level synchronization state.
* **High-level design:** The crate uses a modular architecture where the `Engine` acts as the primary orchestrator. It delegates specialized tasks to sub-components like the `ChainManager`, `Mempool`, `Consensus`, and `PayloadBuilder`. It relies heavily on traits defined in `wasix_eth_types` for cross-crate communication.
* **Fit:** It sits at the core of the project, using `wasix_eth_storage` for persistence and `wasix_eth_execution` for EVM state transitions. It is used by `wasix_eth_rpc` to fulfill Engine API requests and by `wasix_eth_app` as the main node logic.

---

## Module Structure

```text
src/
├── lib.rs              # Crate root and re-exports
├── engine/             # Engine API and Block Production
│   ├── mod.rs
│   ├── engine.rs       # Main Engine orchestrator
│   ├── api.rs          # Engine API trait implementation
│   ├── payload_builder.rs # Block production logic
│   ├── payload_processor.rs # Block import and validation logic
│   └── ...             # Reorg and tracker utilities
├── chain_manager.rs    # Chain state and canonicality management
├── consensus.rs        # Consensus rule implementations (PoS)
├── mempool/            # Transaction pool logic
│   ├── mod.rs
│   ├── mempool.rs      # Internal mempool storage and ordering
│   └── mempool_provider.rs # Public mempool interface
├── account_manager.rs  # State-aware nonce and balance tracking
├── gossip/             # Block and transaction broadcasting
└── sync/               # Synchronization orchestration
```

---

## Public API

### `Engine` (`engine/engine.rs`)
The primary orchestrator for the Ethereum Execution Client.

**Responsibilities:**
- Implementing `SyncProvider` and handling gossip.
- Managing `forkchoiceUpdated` and `newPayload` Engine API calls.
- Coordinating between storage, execution, and consensus.

**Important Methods:**
- `new(...)`: Constructs a new Engine with all its dependencies.
- `forkchoice_updated(...)`: Handles consensus client chain updates and payload building.
- `new_payload(...)`: Validates and imports new execution payloads.
- `import_block(...)`: High-level block import.

### `ChainManager` (`chain_manager.rs`)
Manages the view of the blockchain.

**Responsibilities:**
- Tracking the current canonical head.
- Managing "invalid" blocks and their reasons.
- Resolving chain reorganizations (reorgs).

**Important Methods:**
- `head_block()`: Returns the current canonical head hash and number.
- `add_invalid_block(...)`: Marks a block (and its descendants) as invalid.
- `resolve_reorg(...)`: Calculates common ancestor and context for a switch.

### `Mempool` (`mempool/mempool.rs`)
Manages pending transactions.

**Responsibilities:**
- Validating incoming transactions.
- Ordering transactions by priority fee and nonce.
- Evicting low-value transactions when at capacity.

**Important Methods:**
- `add_transaction(...)`: Adds a transaction to the pool.
- `peek_best_transactions(...)`: Selects optimal transactions for a new block.

---

## Internal Architecture

* **Major Components:**
    * **Engine Orchestrator:** Ties together all subsystems.
    * **Payload Builder:** A background-capable component that constructs blocks from mempool transactions.
    * **Payload Processor:** Handles the complex logic of validating block parents, executing transactions, and managing sidechains/invalid branches.
    * **Chain Manager:** Acts as the source of truth for the chain's structure.
* **Data Flow:**
    1. **Transactions:** RPC/Gossip -> Mempool -> Payload Builder.
    2. **Blocks:** Engine API (`newPayload`) -> Payload Processor -> Execution -> Storage.
    3. **Consensus:** Engine API (`forkchoiceUpdated`) -> Engine -> Chain Manager -> Canonical State Update.
* **Ownership Model:** Components are typically wrapped in `Arc` and shared across multiple tasks (e.g., RPC handlers and background gossip loops).
* **Synchronization:** Uses `tokio::sync::RwLock` and `Mutex` for shared state. The `Engine` and `Mempool` are designed to be thread-safe for concurrent RPC access.

---

## Dependency Graph

| Dependency | Purpose |
| ---------- | ------- |
| `wasix_eth_types` | Shared types, traits, and protocol definitions |
| `wasix_eth_storage` | Persistence layer access |
| `wasix_eth_execution`| EVM execution for blocks and transactions |
| `wasix_eth_utils` | Metrics and logging macros |
| `tokio` | Async runtime and synchronization |
| `alloy-*` | Ethereum primitive types and consensus structures |

---

## Important Types

### `PayloadBuilder` (`engine/payload_builder.rs`)
- **Role:** Dynamically builds blocks while respecting gas limits and priority fees.
- **Lifecycle:** Triggered by `forkchoiceUpdated`. Builds an empty block immediately, then progressively improves it with transactions.

### `PayloadStatus` (`wasix_eth_types`)
- **Role:** Communication of block validity back to the consensus client (VALID, INVALID, SYNCING, ACCEPTED).

### `ReorgContext` (`chain_manager.rs`)
- **Role:** Encapsulates the details of a chain switch, including the common ancestor and the branches to revert/apply.

---

## Execution Flow (New Payload)

1. **API entry point:** `new_payload` is called via Engine API.
2. **Validation:** `PayloadProcessor` checks if the parent is known and valid.
3. **Execution:** Calls `execution_provider.execute_block()` to run EVM state transitions.
4. **State updates:** Writes the resulting state and block data to storage via a batch writer.
5. **Canonicality:** If the block is part of the canonical chain, the `ChainManager` updates the head.
6. **Output:** Returns `VALID` or `INVALID` to the caller.

---

## Error Handling

* **Result Usage:** Heavily uses `wasix_eth_types::Result` (alias for `anyhow::Result`).
* **Invalid Blocks:** Blocks that fail execution or consensus rules are recorded in the `ChainManager` to prevent repeated processing of invalid branches.
* **Logging:** Extensive use of `debug!` and `info!` for tracing Engine API interactions and sync progress.

---

## Configuration

* **Consensus Rules:** Injected via `Arc<dyn Consensus>`, allowing for different consensus mechanisms (e.g., dev-mode vs production PoS).
* **Mempool Capacity:** Configurable limits on the number of pending transactions.
* **Chain Config:** Provided via `wasix_eth_types::ChainConfig`.

---

## Testing

* **Unit Tests:** Found in `mempool.rs`, `consensus.rs`, and `engine.rs`.
* **Integration Tests:** Likely in the top-level `tests/` directory, exercising the full Engine API flow.
* **Mocks:** Extensive use of mocks for `ExecutionProvider` and `Storage` in unit tests to isolate core logic.

---

## Extension Points

* **Payload Building Strategy:** The `PayloadBuilder` can be extended with different transaction selection algorithms (e.g., MEV-aware).
* **Consensus Logic:** New consensus rules can be implemented by fulfilling the `Consensus` trait.
* **Mempool Listeners:** Components can subscribe to `MempoolListener` to be notified of new transactions.

---

## Known Limitations

* **Sidechain Depth:** Tracking of very deep sidechains might impact memory if not periodically pruned.
* **Sync Dependencies:** Some Engine API calls depend on being "synced" to the network, which is tracked by the `ChainManager`.

---

## Cross-Crate Relationships

* **Depends on:** `wasix_eth_storage`, `wasix_eth_execution`, `wasix_eth_types`, `wasix_eth_utils`.
* **Used by:** `wasix_eth_rpc` (to implement Engine/Eth/Admin methods) and `wasix_eth_app`.

---

## Developer Notes

* **Forkchoice Updates:** `forkchoiceUpdated` is the most critical method for chain state. It triggers head updates and potentially initiates payload building for the next block.
* **Async Invariants:** Ensure that locks are not held across `.await` points to avoid deadlocks in the async runtime.
* **Invalidation Propagation:** When a block is marked `INVALID`, the `ChainManager` automatically ensures that no descendant of that block can ever be considered `VALID`.

---

## Summary

`wasix_eth_core` is the heart of the WASIX Ethereum node. It manages the complex interplay between transactions, blocks, and consensus. By centralizing the Engine API and chain state management, it provides a robust and modular foundation for building a modern Ethereum execution client. Its design emphasizes separation of concerns between block production, block validation, and chain state tracking.
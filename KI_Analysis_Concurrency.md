### Refactoring and Concurrency Improvement Plan

Based on the analysis of the `wasix-based-evm` project, the following plan outlines the next steps for refactoring and improving the client's concurrency and maintainability.

#### 1. Concurrency Improvements: Read-Write Separation
The current architecture uses a single `Arc<Mutex<InMemoryStorage>>`, causing significant lock contention.

*   **Action:** Transition `InMemoryStorage` from `Mutex` to `RwLock`.
    *   **Implementation:** Replace `tokio::sync::Mutex` with `tokio::sync::RwLock` in `AppBuilder`, `DefaultBlockchainProvider`, and the networking layer.
    *   **Benefit:** Allows multiple RPC read requests (e.g., `eth_getBalance`, `eth_blockNumber`) to proceed in parallel without waiting for each other, while still ensuring exclusive access for state-modifying operations (block inclusion).

#### 2. Granular State Management: Mempool Decoupling
The `Mempool` is currently an internal field of `InMemoryStorage`, meaning every transaction gossip or `eth_sendTransaction` requires the global storage lock.

*   **Action:** Move `Mempool` out of `InMemoryStorage` into its own `Arc<RwLock<Mempool>>`.
    *   **Implementation:**
        1. Remove the `mempool` field from `InMemoryStorage`.
        2. Update `AppBuilder` to initialize and inject a separate mempool.
        3. Update `DefaultBlockchainProvider` to hold an `Arc<RwLock<Mempool>>`.
    *   **Benefit:** Dramatically reduces contention during high-volume transaction gossip, as the mempool can be updated without touching the main blockchain state.

#### 3. Domain Logic and API Refinement
The `BlockchainProvider` transition is partially complete but can be further isolated from RPC transport concerns.

*   **Action:** Finalize Interface Segregation and Mapping.
    *   **Implementation:**
        1. Update `EthWriteProvider` to use domain-pure transaction requests instead of `tonic::evm_rpc::TransactionRequest`.
        2. Move all remaining protobuf-to-domain conversion logic into `src/rpc/mappers.rs`.
        3. Ensure the `EngineProvider` uses strong types (e.g., `B256` for Payload IDs) instead of `String` keys.
    *   **Benefit:** Makes the core business logic completely independent of gRPC, facilitating the addition of a JSON-RPC 2.0 interface in the future.

#### 4. Advanced Executor Patterns
The `Executor` currently runs synchronously, potentially blocking the async reactor during complex block processing.

*   **Action:** Async Execution Wrappers.
    *   **Implementation:** In the provider layer, wrap heavy execution calls (`execute_block`, `propose_block`) in `tokio::task::spawn_blocking`.
    *   **Benefit:** Prevents long-running EVM executions from starving the network and RPC tasks.

#### 5. Persistence Strategy (Next Milestone)
As identified in the project analysis, moving beyond `InMemoryStorage` is critical for a "regular" client.

*   **Action:** Define a `Database` Trait.
    *   **Implementation:** Create a trait for key-value storage that can be implemented by a WASI-compatible store (e.g., a filesystem-backed simple DB) or `InMemoryBackend` for testing.
    *   **Benefit:** Provides a clear path toward persistence while maintaining the flexibility of the current WASIX environment.

### Summary of Immediate Next Tasks
1.  **High:** Replace `Mutex` with `RwLock` for `InMemoryStorage` in `app.rs` and `provider.rs`.
2.  **High:** Decouple `Mempool` from `InMemoryStorage` into a standalone component.
3.  **Medium:** Wrap EVM execution in `spawn_blocking` within the `EngineProvider` implementation.
4.  **Medium:** Refine `PendingPayload` IDs to use strong types instead of `String`.
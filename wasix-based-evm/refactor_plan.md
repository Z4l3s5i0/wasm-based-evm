### Project Structure Analysis

The current project structure is undergoing a refactor towards a domain-driven, layered architecture (**Storage > Service > Mapper > Controller**).

*   **`src/rpc/`**: Contains the new RPC structure.
    *   **Controllers**: `account_controller.rs`, `block_controller.rs` define `jsonrpsee` traits.
    *   **Services**: `account_service.rs`, `block_service.rs` handle business logic.
    *   **Mappers**: `account_mapper.rs` (and others planned) for converting domain types to RPC types.
    *   **Facade**: `mod.rs` acts as a registry for all RPC modules.
*   **`src/storage/`**: Currently contains a monolithic `InMemoryStorage` based on `evm::backend::InMemoryBackend`.
    *   **`storage.rs`**: Manages state, blocks, transactions, and receipts.
    *   **`genesis.rs`**: Handles chain initialization from an `alloy-genesis` file.
*   **`src/executor.rs`**: Likely contains the EVM execution logic (linked to `storage`).
*   **`src/app.rs`**: The main application entry point that wires together storage, executor, and the RPC server.

---

### Relevant Alloy Libraries for Storage and RPC

To align with the `alloy` ecosystem for better compatibility and serializability, the following traits and structs are essential:

#### 1. `alloy-consensus`
*   **`Header`**: Essential for block storage and `eth_getBlockBy*` RPC calls.
*   **`Transaction` (Trait)**: Defines common transaction properties.
*   **`Receipt`**: Used for storing transaction outcomes and `eth_getTransactionReceipt`.
*   **`Block`**: Aggregates header and transactions.

#### 2. `alloy-primitives`
*   **`Address`, `B256`, `U256`**: The bread and butter for any storage key or value (accounts, hashes, balances).
*   **`Bytes`**: For contract code and transaction input data.
*   **`keccak256`**: For hashing and calculating roots.

#### 3. `alloy-genesis`
*   **`Genesis`**: Already used to define the initial state.
*   **`GenesisAccount`**: Structure for pre-funded accounts and contract code.

#### 4. `alloy-rlp`
*   **`Encodable` / `Decodable`**: Crucial for persisting data to storage in a standard format and for RLP-encoded RPC responses (like `eth_sendRawTransaction`).

#### 5. `alloy-trie`
*   **`TrieAccount`**: Necessary for calculating the state root and verifying proofs.
*   **`HashBuilder`**: For constructing Merkle Patricia Tries.

#### 6. `alloy-eips`
*   **`BlockNumberOrTag`**: Essential for RPC methods that accept `"latest"`, `"earliest"`, or a hex number.
*   **`BlockId`**: Combination of hash or number.

#### 7. `alloy-serde`
*   **`JsonStorageKey`**: Helper for JSON-RPC serialization.
*   **`NumVariants`**: Standardizing hex/decimal representations in RPC.

---

### Storage Restructuring Plan

To fit the new RPC structure, the storage should be decoupled from the EVM's internal representation and split into domain-specific "Stores":

#### 1. Domain-Specific Stores (The "Storage" Layer)
Instead of one `InMemoryStorage` struct, implement a trait-based approach or specialized sub-structs:
*   **`StateStore`**: Handles `Address -> Account` (balance, nonce, code, storage slots). This should integrate with `alloy-trie` for root calculation.
*   **`ChainStore`**: Handles `u64 -> BlockHeader`, `B256 -> BlockHeader`, and canonical chain tracking (Head, Safe, Finalized).
*   **`TransactionStore`**: Handles `B256 -> (Transaction, Receipt, BlockContext)`.

#### 2. Storage as a Provider
The storage layer should implement a "Provider" trait that the RPC Services consume.
```rust
pub trait BlockchainProvider: Send + Sync {
    fn get_account(&self, address: Address, block: BlockId) -> Result<Option<Account>>;
    fn get_block_by_number(&self, num: u64) -> Result<Option<Block>>;
    // ... etc
}
```

#### 3. Transition to Alloy Types
Currently, `InMemoryStorage` uses `evm::backend` types (like `H160`, `EvmU256`). These should be replaced with `alloy_primitives` counterparts (`Address`, `U256`) to avoid constant conversion when mapping to RPC responses.

---
### Extensive Refactoring Plan for RPC and Storage

This plan expands on the previous strategy by incorporating advanced considerations such as state versioning, indexing, and specialized storage providers, while maintaining a clean separation between **Storage**, **Service**, **Mapper**, and **Controller** layers.

---

### 1. Enhanced Storage Layer (The "Provider" Pattern)

The current `InMemoryStorage` will be decomposed into specialized stores that implement granular traits. This allows services to only depend on the data they need.

#### A. Multi-Store Architecture
*   **`StateStore`**:
    *   **Responsibility**: Manages `Address -> Account` mappings and contract storage.
    *   **Versioning**: Implement a `Map<BlockNumber, StateSnapshot>` or a "Delta-based" history to support queries at specific block heights (e.g., `eth_getBalance(addr, "0x123")`).
    *   **Trie Integration**: Uses `alloy-trie` to maintain the state root for each block.
*   **`BlockchainStore`**:
    *   **Responsibility**: Manages the canonical chain.
    *   **Indices**:
        *   `Number -> Hash` (Canonical chain map).
        *   `Hash -> Header`.
        *   `Hash -> BlockBody`.
*   **`TransactionStore`**:
    *   **Responsibility**: Fast lookup for transaction metadata.
    *   **Indices**:
        *   `TxHash -> (BlockNumber, BlockHash, TransactionIndex)`.
        *   `TxHash -> Receipt`.
*   **`FilterStore`**:
    *   **Responsibility**: Efficient log filtering (`eth_getLogs`).
    *   **Index**: `Topic -> Vec<Location>` and `Address -> Vec<Location>`.

#### B. Unified Provider Trait
```rust
pub trait BlockProvider: Send + Sync {
    fn header(&self, id: BlockId) -> Result<Option<Header>>;
    fn block(&self, id: BlockId) -> Result<Option<Block>>;
    fn block_hash(&self, number: u64) -> Result<Option<B256>>;
}

pub trait StateProvider: Send + Sync {
    fn account(&self, address: Address, id: BlockId) -> Result<Option<Account>>;
    fn storage(&self, address: Address, slot: B256, id: BlockId) -> Result<Option<U256>>;
}

pub trait TransactionProvider: Send + Sync {
    fn transaction(&self, hash: B256) -> Result<Option<Transaction>>;
    fn receipt(&self, hash: B256) -> Result<Option<Receipt>>;
}
```

---

### 2. Service Layer Logic

Services will consume these traits to fulfill RPC requests, performing any necessary business logic (like re-executing a call or aggregating data).

*   **`AccountService`**: Uses `StateProvider`. Handles address validation and state lookups at specific `BlockId`s.
*   **`BlockService`**: Uses `BlockProvider`. Resolves tags like `"latest"`, `"safe"`, and `"finalized"` into concrete numbers before querying.
*   **`TransactionService`**: Uses `TransactionProvider` and `BlockProvider` to return full transaction objects including block context.

---

### 3. Mapper Layer (Data Normalization)

Mappers will be pure, stateless functions converting internal `alloy` types to RPC-compliant JSON structures.

*   **`BlockMapper`**: Converts `alloy_consensus::Block` to `alloy_rpc_types::Block`.
*   **`ReceiptMapper`**: Adds derived fields like `effectiveGasPrice` and `cumulativeGasUsed` which are expected by Ethereum clients but might not be stored in the raw receipt.

---

### 4. Controller Layer (The JSON-RPC Interface)

Controllers implement the `jsonrpsee` traits. They are thin wrappers that handle:
1.  **Input Parsing**: Converting hex strings/numbers to `alloy` primitives.
2.  **Service Delegation**: Calling the appropriate async service method.
3.  **Error Mapping**: Converting internal `ProviderError` or `ExecutionError` into standard JSON-RPC error codes (e.g., `-32000`).

---

### 5. Advanced Considerations Added to Plan

#### I. State Versioning & Snapshots
To support `eth_call` or `eth_getBalance` at historical blocks:
*   **Implementation**: Store "State Transitions" (the diffs) for the last $N$ blocks.
*   **Fallback**: If a query is for a block older than $N$, return an "Historical state unavailable" error or implement a full archival node logic.

#### II. Transaction Indexing
The current `InMemoryStorage` maps hash to transaction, but lacks the "Location".
*   **Plan**: Create an index `TransactionHash -> TransactionMeta { block_hash, block_number, index }`. This is essential for `eth_getTransactionByHash` to return the mandatory `blockHash` field.

#### III. Log Filtering (`eth_getLogs`)
Scanning every block for logs is too slow.
*   **Plan**: Implement a `BloomFilter` index in the `Header` (standard Ethereum) and a secondary lookup table in storage that maps `Address` and `Topic` to a bitset of block numbers containing matching logs.

#### IV. Concurrency & Performance
*   **Pattern**: Use `Arc<RwLock<...>>` for the stores, but ensure that "Read" operations in the RPC don't block "Write" operations from the Executor for too long.
*   **Optimization**: Use `dashmap` or atomic-based caches for frequently accessed data like the "Latest Block Hash".

#### V. Error Handling Strategy
Create a centralized `RpcError` enum:
```rust
pub enum RpcError {
    BlockNotFound(BlockId),
    StateUnavailable(u64),
    InvalidParams(String),
    Internal(String),
}

impl From<RpcError> for jsonrpsee::types::ErrorObjectOwned {
    fn from(err: RpcError) -> Self {
        // Map to standard JSON-RPC codes
    }
}
```

---

### Summary of the Refactored Flow

1.  **App** initializes `Storage` (split into State, Chain, and Tx stores).
2.  **App** creates **Services**, injecting the `Arc<Storage>` (as traits).
3.  **App** registers **Controllers** into the **Facade**.
4.  **Facade** starts the `jsonrpsee` server.
5.  **Request** arrives -> **Controller** parses -> **Service** fetches from **Storage** -> **Mapper** formats -> **Response** sent.
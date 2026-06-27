# `wasix_eth_storage`

## Overview

* **Purpose:** The `wasix_eth_storage` crate provides a high-performance, persistent storage layer for the WASIX Ethereum node. It abstracts the underlying MDBX database and manages the complex data structures required for an Ethereum execution client.
* **Responsibilities:**
    * Managing persistent storage of blocks, headers, transactions, and receipts.
    * Maintaining the Ethereum state (accounts and contract storage).
    * Implementing a custom Merkle Patricia Trie (MPT) for state root calculation.
    * Handling database transactions and atomic batch writes.
    * Providing RLP-based serialization/deserialization for all stored data types.
* **High-level design:** The crate uses a trait-based provider pattern. It separates read operations (`DatabaseReadProvider`) from write operations (`DatabaseWriteProvider` and `BatchWriter`). It defines a structured schema of tables (via `EthDatabase`) and uses RLP codecs for efficient data storage.
* **Fit:** It is the foundational persistence layer used by `wasix_eth_core`, `wasix_eth_execution`, and `wasix_eth_rpc`.

---

## Module Structure

```text
src/
├── lib.rs              # Crate root and public trait exports
├── db.rs               # Core MDBX database management (EthDatabase)
├── tables.rs           # Database table definitions
├── codecs.rs           # RLP serialization logic
├── read.rs             # Read provider implementation (DatabaseReadProvider)
├── read_traits.rs      # Fine-grained read interfaces
├── write.rs            # Write provider and batching (BatchWriter)
├── write_traits.rs     # Fine-grained write interfaces
└── trie.rs             # Merkle Patricia Trie implementation
```

---

## Public API

### `EthDatabase` (`db.rs`)
The owner of the physical database.

**Responsibilities:**
- Opening/Closing the MDBX database.
- Initializing tables and genesis state.
- Spawning read and write transactions.

**Important Methods:**
- `open(path)`: Opens the database at the specified path.
- `init_genesis(config)`: Bootstraps the database with genesis data.
- `begin_read()` / `begin_write()`: Returns low-level MDBX transaction handles.

### `DatabaseReadProvider` (`read.rs`)
The primary interface for non-mutating database access.

**Responsibilities:**
- Implementing all `read_traits` (Header, Block, Account, etc.).
- Providing high-level query methods.

### `BatchWriter` (`write.rs`)
A transactional container for atomic updates.

**Responsibilities:**
- Buffering state changes (accounts, storage) in memory.
- Calculating the state root before committing.
- Implementing all `write_traits`.

**Important Methods:**
- `commit()`: Flushes all changes to the persistent storage.
- `calculate_state_root()`: Computes the Merkle root of the current state.

---

## Internal Architecture

* **Major Components:**
    * **MDBX Wrapper:** Low-level integration with the `libmdbx` database.
    * **MPT (Merkle Patricia Trie):** A custom implementation in `trie.rs` that supports recursive insertion, deletion, and hashing.
    * **Provider Layer:** Decouples the storage implementation from the rest of the node.
* **Data Flow:**
    1. **Read:** Components use `DatabaseReadProvider` to fetch data.
    2. **Write:** Components use `BatchWriter` to stage changes.
    3. **Trie Update:** During `commit`, the `BatchWriter` updates the state trie in `trie.rs`.
    4. **Persistence:** The underlying MDBX transaction is committed, making changes durable.
* **Ownership Model:** `EthDatabase` is usually wrapped in an `Arc`. Providers borrow a reference to the inner database handle.
* **Concurrency:** MDBX supports multiple concurrent readers and a single writer. `BatchWriter` ensures that only one write operation happens at a time per node instance.

---

## Dependency Graph

| Dependency | Purpose |
| ---------- | ------- |
| `wasix_eth_types` | Shared Ethereum data types |
| `mdbx-rt` | Rust bindings for MDBX database |
| `alloy-rlp` | RLP encoding/decoding |
| `alloy-primitives`| Fast B256/U256 types |
| `anyhow` | Error management |

---

## Execution Flow (Block Storage)

1. **Transaction Start:** `DatabaseWriteProvider` creates a `BatchWriter`.
2. **Data Insertion:** `BatchWriter` inserts headers, transactions, and receipts into their respective tables.
3. **State Update:** EVM changes are applied via `update_account` and `update_storage`.
4. **Root Calculation:** `calculate_state_root` is called, which walks the trie and updates nodes.
5. **Finalization:** `commit()` is called, ensuring all data is persisted atomically.

---

## Error Handling

* **Result Type:** Uses `anyhow::Result` for propagation of database and trie errors.
* **Database Errors:** Maps MDBX-specific errors to high-level error messages.
* **Trie Invariants:** Errors are raised if state roots do not match during verification.

---

## Configuration

* **Table Schema:** Hardcoded in `db.rs` and `tables.rs`.
* **Pathing:** Configured via the `data_dir` CLI argument in `wasix_eth_app`.

---

## Testing

* **Unit Tests:** Found in `db.rs` and `trie.rs` for verifying encoding and trie logic.
* **Integration Tests:** Verifies full block write/read cycles.
* **In-Memory Mocking:** `MemoryState` in `trie.rs` allows testing trie logic without persistent storage.

---

## Extension Points

* **New Tables:** Can be added by defining a struct in `tables.rs` and updating `init_tables` in `db.rs`.
* **Custom Codecs:** Different serialization formats can be added to `codecs.rs`.
* **Alternative Storage:** New providers can implement the traits in `read_traits.rs` and `write_traits.rs`.

---

## Known Limitations

* **State Root Performance:** State root calculation can be intensive; the current trie implementation is optimized for correctness but may have room for performance tuning.
* **Disk Space:** Ethereum nodes require significant disk space; pruning strategies are not yet explicitly visible in the current implementation.

---

## Cross-Crate Relationships

* **Depends on:** `wasix_eth_types`.
* **Used by:** `wasix_eth_core`, `wasix_eth_execution`, `wasix_eth_rpc`, `wasix_eth_p2p`.

---

## Developer Notes

* **Table Keys:** Keys in the database are often hashed (e.g., `HashedState`) or composite (e.g., `Receipts` keyed by block hash and index).
* **Batching:** Always use `BatchWriter` for multi-step updates to ensure atomicity and avoid partial state transitions.
* **RLP:** Data is stored in RLP format. Ensure any type added to the database implements `Encodable` and `Decodable` from `alloy-rlp`.

---

## Summary

`wasix_eth_storage` is a robust and structured persistence layer. By leveraging MDBX and a custom Merkle Patricia Trie, it provides the necessary performance and reliability for Ethereum state management. Its trait-based design allows for flexible integration across the entire node architecture while maintaining clear boundaries between read and write operations.
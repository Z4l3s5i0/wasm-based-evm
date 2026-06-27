# `wasix_eth_types`

## Overview

* **Purpose:** The `wasix_eth_types` crate is the central repository for all shared data types, traits, and constants used across the WASIX Ethereum node. It ensures type consistency between the storage, networking, execution, and RPC layers.
* **Responsibilities:**
    * Re-exporting and extending core Ethereum types from the `alloy` ecosystem.
    * Defining internal service traits (e.g., `GossipProvider`, `SyncProvider`).
    * Implementing the Ethereum wire protocol (`eth` and `snap`) message structures.
    * Centralizing JSON-RPC server and client trait definitions.
    * Managing hardfork activation logic and genesis configurations.
    * Providing common error types and result aliases.
* **High-level design:** The crate is designed as a "thin" dependency that mostly contains data structures (Plain Old Data) and traits. It minimizes heavy business logic to keep compile times low for dependent crates. It heavily leverages `alloy-primitives` and `alloy-consensus` to stay compatible with the modern Rust Ethereum ecosystem.
* **Fit:** This is the most depended-upon crate in the project. Almost every other `wasix_eth_*` crate depends on it.

---

## Module Structure

```text
src/
├── lib.rs              # Root: Re-exports, core traits, Hardfork logic, and Receipts
├── admin.rs            # Admin API structures and trait
├── engine_types.rs     # Engine API versioned execution payloads
├── error.rs            # RpcError and Result types
├── eth.rs              # Standard Ethereum JSON-RPC trait (EthRpc)
├── genesis.rs          # Genesis configuration and account definitions
├── p2p.rs              # Ethereum wire protocol (eth/66-68) and Snap protocol messages
└── sync.rs             # Cross-crate synchronization and peer provider traits
```

---

## Public API

### `Hardfork` (`lib.rs`)
Enum representing Ethereum hardforks from Frontier to Prague.

**Responsibilities:**
- Determining the active fork based on block number and timestamp.
- Providing fork-specific parameters (e.g., blob parameters).

### `EthRpc` (`eth.rs`)
The `jsonrpsee` server trait for standard Ethereum methods.

**Responsibilities:**
- Defining the interface for `eth_*` methods like `eth_gasPrice`, `eth_sendRawTransaction`, etc.

### `P2pSession` (`sync.rs`)
Trait for an active peer-to-peer connection.

**Responsibilities:**
- Defining methods for requesting headers, bodies, receipts, and transactions from a peer.

### `ForkId` (`p2p.rs`)
Implementation of EIP-2124 Fork Identifiers.

**Responsibilities:**
- Generating and validating fork IDs used during the P2P handshake to ensure chain compatibility.

---

## Internal Architecture

* **Major Components:**
    * **Provider Traits:** Define the interface for components like the Mempool, Gossip, and Sync services without introducing circular dependencies.
    * **Protocol Messages:** RLP-encodable structs for every message in the `eth` wire protocol.
    * **RPC Envelopes:** Versioned payloads (V1-V4) for the Engine API to support multiple hardforks (Merge, Shanghai, Cancun, Prague).
* **Ownership Model:** Mostly uses simple ownership of data. Traits are designed to be implemented by types wrapped in `Arc`.
* **Async Architecture:** Uses the `async_trait` macro for all provider and session interfaces to support `tokio`-based async execution in higher-level crates.

---

## Dependency Graph

| Dependency | Purpose |
| ---------- | ------- |
| `alloy-primitives` | Foundational types: `Address`, `B256`, `U256` |
| `alloy-consensus` | Standard block and transaction structures |
| `alloy-rlp` | RLP encoding/decoding for database and wire protocol |
| `alloy-rpc-types` | Base types for JSON-RPC responses |
| `jsonrpsee` | RPC framework macros and error types |
| `thiserror` | Ergonomic error definition for `RpcError` |

---

## Important Types

### `Receipt` (`lib.rs`)
Custom implementation of an Ethereum transaction receipt.
- **Role:** Stores the result of a transaction execution.
- **Lifecycle:** Created by the Executor, stored in the Database.
- **Interactions:** RLP encoded/decoded during block import and export.

### `ExecutionPayloadV1-V4` (`engine_types.rs`)
Evolutionary types for the Engine API.
- **Invariants:** Each version extends the previous one to include new fields like `withdrawals` (V2) or `blobGasUsed` (V3).

### `GossipMessage` (`p2p.rs`)
Enum wrapping all possible messages that can be gossiped or requested over the wire.

---

## Error Handling

* **`RpcError`:** A comprehensive enum in `error.rs` using `thiserror`. It covers everything from "Block Not Found" to "Invalid JWT Token".
* **`RpcResult<T>`:** A type alias for `Result<T, RpcError>`.
* **Conversion:** Implements `From<RpcError>` for `jsonrpsee::types::ErrorObjectOwned`, allowing internal errors to be returned directly to RPC clients with appropriate JSON-RPC error codes.

---

## Configuration

* **`ChainConfig`:** (Re-exported from `alloy-genesis`) Defines hardfork activation heights/times.
* **`GenesisConfiguration`:** Struct in `genesis.rs` for parsing `genesis.json` files, including the initial allocation of ether to accounts.

---

## Testing

* **Unit Tests:** Found in `lib.rs` and `p2p.rs` for verifying:
    * Hex serialization/deserialization.
    * Fork ID generation and validation.
    * RLP encoding of custom `Receipt` types.
* **Mocks:** Provides `NoopGossip` and `NoopSync` (in `sync.rs`) to allow other crates to be tested without functional networking or sync logic.

---

## Extension Points

* **New Hardforks:** Add a variant to the `Hardfork` enum and update `get_active_fork` in `lib.rs`.
* **New RPC Namespaces:** Create a new module (like `admin.rs`) with a `#[rpc(server)]` trait.
* **Wire Protocol Updates:** Add new message structs to `p2p.rs` and update `EthMessageID`.

---

## Known Limitations

* **Manual Codecs:** Some types (like `Receipt`) have manually implemented RLP codecs instead of derived ones to match specific Ethereum edge cases.
* **Type Coupling:** While it minimizes logic, it is the "bottleneck" dependency—changing common types here requires recompiling the entire project.

---

## Cross-Crate Relationships

* **Base for all:** `wasix_eth_storage`, `wasix_eth_p2p`, `wasix_eth_core`, `wasix_eth_rpc`, and `wasix_eth_app` all depend on this crate.
* **Interface provider:** It defines the traits that allow `wasix_eth_rpc` to call into `wasix_eth_core` without a direct dependency.

---

## Developer Notes

* **Alloy Compatibility:** When adding new fields, check if `alloy-rpc-types` already has a definition to maintain ecosystem compatibility.
* **Hex Prefix:** Use `parse_strict_hex` in `error.rs` when deserializing RPC parameters to ensure the `0x` prefix is present, as required by many Ethereum clients.

---

## Summary

`wasix_eth_types` is the "glue" of the project. It centralizes the Ethereum domain model, wire protocol definitions, and cross-component interfaces. By relying heavily on the `alloy` suite and provide-based traits, it provides a stable and modern foundation for building a modular Ethereum execution client.
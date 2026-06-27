# `wasix_eth_execution`

## Overview

* **Purpose:** The `wasix_eth_execution` crate is responsible for executing Ethereum transactions and blocks. It acts as the bridge between the high-level blockchain logic and the low-level EVM (Ethereum Virtual Machine).
* **Responsibilities:**
    * Orchestrating block execution and state transitions.
    * Integrating with the SputnikVM (EVM implementation).
    * Handling Ethereum hardfork logic and configuration.
    * Managing state application (committing EVM changes to the database).
    * Processing system-level operations like block rewards, withdrawals, and beacon root updates.
    * Supporting "Payload Building" for consensus engines.
* **High-level design:** The crate uses a provider pattern (`EthExecutionProvider`) as the main entry point. It wraps an `EvmExecutor` which handles the actual EVM invocations. It uses a custom `SputnikBackend` to map EVM state requests to the project's specialized storage layer.
* **Fit:** It sits between `wasix_eth_core` (which handles consensus and engine API) and `wasix_eth_storage`. It depends on `wasix_eth_types` for shared data structures.

---

## Module Structure

```text
src/
├── lib.rs
├── execution_provider.rs # Main entry point (EthExecutionProvider)
├── executor.rs           # EVM execution orchestration (EvmExecutor)
├── backend.rs            # EVM backend traits implementation (SputnikBackend)
├── transaction.rs        # Transaction validation and execution logic
├── state.rs              # State transition application (StateApplier)
├── block.rs              # Block-level operations (BlockProcessor)
└── config.rs             # Hardfork and environment configuration
```

---

## Public API

### `EthExecutionProvider` (`execution_provider.rs`)
The primary high-level API for executing blocks.

**Responsibilities:**
- Executing full blocks or building new payloads.
- Managing database batches for atomic state updates.
- Calculating post-execution fields (receipts root, gas used, logs bloom).

**Important Methods:**
- `execute_block()`: Executes all transactions in a block and returns receipts.
- `execute_block_for_payload()`: Executes transactions for a new block being built.

### `EvmExecutor` (`executor.rs`)
Low-level wrapper for EVM execution.

**Responsibilities:**
- Executing individual transactions.
- Performing "System Calls" (e.g., EIP-7002, EIP-2935).

**Important Methods:**
- `execute_transaction()`: Runs a single transaction through the EVM.
- `execute_system_call()`: Executes a gasless system call to a predeployed contract.

### `SputnikBackend` (`backend.rs`)
Implementation of `evm::backend::RuntimeBackend`.

**Responsibilities:**
- Providing the EVM with access to accounts, code, and storage from the database.
- Managing transient storage (EIP-1153).
- Tracking "hot/cold" access for gas calculations (EIP-2929).

### `TransactionExecutor` (`transaction.rs`)
Handles the lifecycle of a single transaction execution.

---

## Internal Architecture

* **Major Components:**
    * **Execution Provider:** The coordinator.
    * **Executor:** The EVM invoker.
    * **SputnikBackend:** The state interface.
    * **State Applier:** The database committer.
* **Data Flow:**
    1. `EthExecutionProvider` receives a block.
    2. It creates a `BatchWriter` for the database.
    3. It initializes the EVM environment (gas limit, block number, etc.) via `config.rs`.
    4. For each transaction, `TransactionExecutor` is used to:
        - Validate (nonce, balance, gas).
        - Execute in SputnikVM via `SputnikBackend`.
        - Collect logs and calculate gas used.
    5. `StateApplier` writes the resulting `OverlayedChangeSet` back to the `BatchWriter`.
    6. Finally, block-level logic (rewards, withdrawals) is applied via `BlockProcessor`.
* **Ownership Model:** `EthExecutionProvider` holds `Arc` references to storage providers. During execution, it creates short-lived executors and backends that borrow these providers.
* **Synchronization:** Execution is generally synchronous per-block. Concurrency is handled at higher levels (e.g., processing multiple blocks in different sync stages).

---

## Dependency Graph

| Dependency | Purpose |
| ---------- | ------- |
| `wasix_eth_types` | Core Ethereum types (Block, Transaction, Address, etc.) |
| `wasix_eth_storage` | Database read/write access (MDBX) |
| `wasix_eth_utils` | Metrics and logging |
| `evm` | SputnikVM core (EVM implementation) |
| `evm-precompile` | Standard Ethereum precompiles |
| `alloy-rlp` | RLP encoding/decoding |
| `anyhow` | Error handling |

---

## Execution Flow (Block Execution)

1. **Environment Setup:** `prepare_execution_env` determines the active hardfork and sets EVM parameters.
2. **System Inits:** `BlockProcessor` applies beacon root updates or initializes system contracts (e.g., EIP-4788).
3. **Transaction Loop:**
    - Validate transaction (sender, nonce, balance).
    - Execute transaction in SputnikVM.
    - Generate Receipt and collect Logs.
    - Apply state changes to the local batch.
4. **Finalization:**
    - Process Withdrawals (EIP-4895).
    - Apply Block Rewards (for PoW or specific dev forks).
    - Finalize block header (update state root, gas used, receipts root).

---

## Error Handling

* **Custom Errors:** Uses `anyhow::Result` for flexible error propagation.
* **EVM Errors:** Maps SputnikVM `ExitError` to project-specific logic (e.g., reverting changes but consuming gas).
* **Recoverable Errors:** Transaction-level reverts are captured in receipts and do not stop block execution.
* **Unrecoverable Errors:** Database failures or fundamental consensus violations (e.g., invalid state root) result in `Err`.

---

## Configuration

* **Chain Configuration:** Driven by `ChainConfig` from `wasix_eth_types`, which defines hardfork activation blocks/timestamps.
* **Hardforks:** Supports logic from `Frontier` up to `Prague`.
* **EVM Config:** `get_evm_config` provides SputnikVM with the correct feature set for the active fork.

---

## Testing

* **Unit Tests:** Found in `src/` modules (e.g., gas calculation tests).
* **Integration Tests:** Likely in `tests/` directory (not explored here, but inferred from crate structure).
* **Mocking:** Uses `InMemoryEnvironment` and `OverlayedBackend` to simulate state without immediate persistence.

---

## Extension Points

* **New Hardforks:** Adding a new variant to `Hardfork` and updating `config.rs` and `block.rs`.
* **Custom Precompiles:** Modifying the `invoker` setup in `executor.rs`.
* **Alternative Backends:** Implementing `RuntimeBackend` for different storage engines.

---

## Known Limitations

* **Sequential Execution:** Transactions within a block are executed sequentially.
* **SputnikVM Specifics:** Dependent on SputnikVM's internal architecture and trait implementations.
* **TODOs:** Some "custom adjustments" in `config.rs` are placeholders.

---

## Cross-Crate Relationships

* **Depends on:** `wasix_eth_storage`, `wasix_eth_types`, `wasix_eth_utils`.
* **Used by:** `wasix_eth_core` (for block processing and engine API) and `wasix_eth_app` (for RLP imports).

---

## Developer Notes

* **State Root:** The state root is calculated at the end of block execution by committing the `BatchWriter`. Intermediate state roots for transactions are optional and performance-intensive.
* **Gas Accounting:** Intrinsic gas is calculated manually in `config.rs` to match Ethereum specifications, while execution gas is handled by the EVM.
* **System Calls:** System calls (like EIP-7002) are special gasless transactions that must bypass normal validation and balance checks.

---

## Summary

`wasix_eth_execution` is the engine room of the node. It takes raw transactions, runs them through a compliant EVM (SputnikVM), and applies the resulting state changes to the database while respecting the complex rules of various Ethereum hardforks. Its design focuses on modularity between the EVM implementation, the state backend, and the consensus-level block logic.
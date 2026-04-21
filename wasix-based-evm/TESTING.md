### Unit Testing Plan for Wasix-Based EVM

To ensure all critical components of the wasix-based EVM function correctly, the following unit testing plan focuses on core logic, state management, and communication layers.

#### 1. Core EVM & Execution (`src/evm/executor.rs`)
*   **Transaction Execution**: Test individual transactions (Legacy, EIP-1559, EIP-2930) for successful execution, gas consumption, and state changes.
*   **Block Execution**: Verify that `execute_block` correctly processes multiple transactions, updates the cumulative gas used, and generates valid receipts.
*   **State Root Calculation**: Validate that `calculate_roots` produces the correct Merkle roots (state, transactions, receipts) following execution.
*   **Finalization Logic**: Test `finalize_state` to ensure it correctly persists changes to `InMemoryStorage` and handles block rewards/withdrawals.

#### 2. Storage & State Management (`src/storage/storage.rs`)
*   **Genesis Initialization**: Verify that `new_with_genesis` correctly initializes the state with provided accounts and balances.
*   **Account State**: Test `get_balance`, `set_balance`, and `get_code` across various addresses.
*   **Block History**: Ensure `add_block` and `get_block_by_number/hash` work as expected, maintaining a consistent chain.
*   **Forkchoice Updates**: Validate `update_forkchoice` correctly sets the head, safe, and finalized block markers.

#### 3. Mempool Logic (`src/mempool.rs`)
*   **Validation Rules**: Test transaction admission based on nonce (too low, too high/queued) and base fee (gas price checks).
*   **Promotion**: Verify that `promote_queued` correctly moves transactions to the pending pool once nonces become sequential.
*   **Revalidation**: Test `revalidate` when the base fee changes or state is updated (e.g., after a block is processed).
*   **Eviction**: Ensure `enforce_capacity` correctly drops the lowest-priority transactions when full.

#### 4. Synchronization (`src/sync/controller.rs`, `src/sync/processor.rs`)
*   **Block Processing**: Test `BlockProcessor::process_block` with valid and invalid blocks, ensuring it triggers state updates and mempool cleaning.
*   **Sync Logic**: Mock `PeerManager` to test `SyncController`'s ability to fetch missing ranges of blocks and handle response timeouts.

#### 5. Network & Gossip (`src/p2p/gossip_handler.rs`)
*   **Message Decoding**: Test `handle_message` with RLP-encoded blocks and transactions.
*   **Propagation**: Verify that new transactions are added to the mempool and re-broadcast, while known transactions are ignored.

#### 6. RPC & Account Management (`src/rpc/account_manager.rs`)
*   **Signing**: Test `sign_transaction` and `sign_transaction_1559` to ensure produced signatures are valid and recoverable to the managed address.

---

### Test Driven Design (TDD) Instructions

When implementing new features or fixing bugs in the EVM, follow these TDD steps using the **Ethereum Execution Client Specifications** (located in `execution-specs/`) as the source of truth:

1.  **Consult Specifications**: Before writing code, identify the relevant logic in the Python-based execution specs (e.g., `execution-specs/src/ethereum/forks/cancun/fork.py` for block transition logic).
2.  **Define the Test Case**: Write a failing Rust unit test in the relevant module (e.g., `src/evm/executor.rs`) that replicates the behavior or state transition described in the specs. Use the exact constants (like `GAS_LIMIT_MINIMUM`) and logic flow (like `apply_body` or `process_transaction`) found in the specs.
3.  **Implement Minimum Logic**: Write only the necessary code in Rust to make the test pass, ensuring the implementation mirrors the specification's mathematical and logic-gate requirements.
4.  **Refactor with Confidence**: Once the test passes and aligns with the execution specs, refactor the Rust code for performance or idiomatic clarity while maintaining the established test coverage.
5.  **Edge Case Validation**: Add additional tests for exception handling (e.g., `InsufficientBalanceError`, `NonceMismatchError`) based on the `exceptions.py` definitions in the specs to ensure robust error parity with the reference implementation.
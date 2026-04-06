### Engine API Test Use Cases

To test the newly implemented Ethereum Engine API methods using the `tester` tool, you can follow these documented use cases. These scenarios cover the typical lifecycle of block production and chain management.

---

### Use Case 1: Standard Block Production (The Proposer Flow)
This simulates a consensus client (CL) asking the execution client (EL) to build a new block from pending transactions in the mempool.

1.  **Add transactions to the mempool**:
    ```bash
    send-transaction --from 0xaf349557fca502757fd26ce7dc71c246dee2ec33 --to 0xd384636941d9081844bb23291e545a503823f5c4 --value 100 --nonce 0
    send-transaction --from 0xaf349557fca502757fd26ce7dc71c246dee2ec33 --to 0xe0f5206BBD039e7b0592d8918820024e2a7437b9 --value 50 --nonce 1
    ```

2.  **Trigger block building**:
    Use `engine-forkchoice-updated` with `payload-attributes`. This tells the EVM to start building a block on top of the current head.
    *Note: Get the current head hash first using `get-roots` or `explorer`.*
    ```bash
    engine-forkchoice-updated --head <CURRENT_HEAD_HASH> --timestamp 1712400000 --suggested-fee-recipient 0xaf349557fca502757fd26ce7dc71c246dee2ec33
    ```
    *Response will provide a `payload_id` (e.g., `0x1234...`).*

3.  **Retrieve the built payload**:
    ```bash
    engine-get-payload --payload-id <PAYLOAD_ID>
    ```

4.  **Submit the payload as a new block**:
    Take the details from `engine-get-payload` and submit them back via `engine-new-payload` to "formally" accept the block.
    ```bash
    engine-new-payload --parent-hash <PARENT_HASH> --block-hash <BLOCK_HASH> --block-number <NUMBER> --state-root <STATE_ROOT> ... [other fields]
    engine-new-payload --parent-hash 0xac2a851636a4933ccc8f9e24756076cba96c87cbf52b3686073225d860366d6f --block-hash 0x903dc6d1aa8a1400f7689733ca97910096ad7330454d3e4a890b7a27daf91eea --block-number 1 --state-root 0x0000000000000000000000000000000000000000000000000000000000000000 --fee-recipient 0xaf349557fca502757fd26ce7dc71c246dee2ec33 --receipts-root 0x0000000000000000000000000000000000000000000000000000000000000000 --gas-limit 30000000 --gas-used 21000 --timestamp 1712400000 --base-fee 1000000000 --logs-bloom 0x0000000000000000000000000000000000000000000000000000000000000000 --prev-randao 0x0000000000000000000000000000000000000000000000000000000000000000
    ```

5.  **Finalize the head**:
    Update the fork choice to point to the new block hash.
    ```bash
    engine-forkchoice-updated --head <NEW_BLOCK_HASH> --safe <NEW_BLOCK_HASH> --finalized <NEW_BLOCK_HASH>
    ```

---

### Use Case 2: Chain Reorganization and Rollback
This tests the mempool's ability to recover transactions when a pending block is superseded by a different head.

1.  **Queue transactions**:
    ```bash
    evm> send-transaction --from 0xaf349557fca502757fd26ce7dc71c246dee2ec33 --to 0x... --value 10 --nonce 0
    ```

2.  **Start building (Payload A)**:
    ```bash
    evm> engine-forkchoice-updated --head <HASH_1> --timestamp 1000
    ```
    *Transactions are now moved from Mempool to PendingPayload A.*

3.  **Switch Head (Chain Reorg)**:
    Update the head to a different branch (`HASH_2`).
    ```bash
    evm> engine-forkchoice-updated --head <HASH_2>
    ```
    *The EVM should detect that Payload A is now stale, discard it, and roll its transactions back into the Mempool.*

4.  **Verify**:
    Check if the transactions are back in the mempool.
    ```bash
    evm> get-mempool
    ```

---

### Use Case 3: Payload Validation and Execution
This tests the server's RLP decoding and state transition logic using the `tester`'s local block buffer.

1.  **Prepare a block in the tester**:
    ```bash
    evm> block-start --slot 10 --parent-hash <HEAD_HASH>
    evm> block-add-tx --from 0x... --to 0x... --value 50 --nonce 0
    ```

2.  **Simulate Engine New Payload**:
    The `tester` will locally RLP-encode the transactions in the buffer and calculate the `transactions_root` before sending.
    ```bash
    evm> engine-new-payload --parent-hash <HEAD_HASH> --block-number 10 --timestamp 1712400000 --block-hash <CALCULATED_OR_MOCK_HASH> --state-root <ZERO_OR_MOCK> ...
    ```
    *The server will decode the RLP, execute the transaction, and return `VALID` if the roots match or `INVALID` if execution fails.*

---

### Use Case 4: Fee Market and Priority Testing
Test the mempool's sorting logic (Gas Price vs. Nonce).

1.  **Add transactions out of order**:
    ```bash
    evm> send-transaction --nonce 1 --gas-price 20 --value 10 ...
    evm> send-transaction --nonce 0 --gas-price 10 --value 10 ...
    ```

2.  **Propose a block from mempool**:
    ```bash
    evm> engine-forkchoice-updated --head <HEAD> --timestamp 2000
    evm> engine-get-payload --payload-id <ID>
    ```
    *Verify that the resulting payload contains the transactions in Nonce order (0 then 1), even though they were submitted differently and have different gas prices.*

---

### Use Case 5: Transaction Replacement
Test the "Replace by Fee" (RBF) logic in the mempool.

1.  **Send a low-fee transaction**:
    ```bash
    evm> send-transaction --nonce 5 --gas-price 10 --value 100
    ```

2.  **Replace with a high-fee transaction**:
    ```bash
    evm> send-transaction --nonce 5 --gas-price 50 --value 100
    ```

3.  **Verify**:
    Check the mempool to ensure only the transaction with the higher gas price remains for nonce 5.
    ```bash
    evm> get-mempool
    ```
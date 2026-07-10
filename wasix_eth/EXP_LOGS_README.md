# WASIX-ETH Experiment Logging (EXP Level)

This document describes the `exp` log level and the structured logs added for performance experiments and benchmarking.

## Enabling Experiment Logs

To enable these logs, run the `wasix-eth` node with the `--verbose 3` flag:

```bash
wasix-eth --verbose 3 [other-args]
```

Logs will be printed to stdout and added to the internal log buffer with the `EXP` prefix.

## Structured Log Formats

All experiment logs follow a consistent format: `[EXP] EVENT_NAME key1=value1 key2=value2 ...`

### Transaction Execution

*   **`TX_EXEC_START`**: Triggered when a transaction starts executing in the EVM.
    *   `hash`: Transaction hash.
    *   `sender`: Sender address.
    *   `nonce`: Transaction nonce.
*   **`TX_EXEC_END`**: Triggered when a transaction finishes execution.
    *   `hash`: Transaction hash.
    *   `success`: Boolean indicating if the transaction succeeded or failed (reverted).
    *   `gas_used`: Gas used by this transaction.
    *   `cumulative_gas`: Cumulative gas used in the block so far.
    *   `elapsed_ms`: Time taken to execute the transaction in milliseconds.

### Block Execution

*   **`BLOCK_EXEC_START`**: Triggered when the node begins processing a new block payload.
    *   `number`: Block number.
    *   `hash`: Block hash.
    *   `parent`: Parent block hash.
    *   `tx_count`: Number of transactions in the block.
*   **`BLOCK_EXEC_END`**: Triggered after the block has been executed (but before persistence).
    *   `number`: Block number.
    *   `hash`: Block hash.
    *   `elapsed_ms`: Total time taken to execute all transactions in the block.

### P2P Networking

*   **`P2P_RECV_TX`**: Triggered when a transaction is received via P2P (Gossip).
    *   `peer`: Remote peer ID.
    *   `hash`: Transaction hash.
*   **`P2P_SEND_TX`**: Triggered when a transaction is broadcasted to a peer.
    *   `peer`: Remote peer ID.
    *   `hash`: Transaction hash.
*   **`P2P_RECV_BLOCK`**: Triggered when a new block is received via P2P (NewBlock message).
    *   `peer`: Remote peer ID.
    *   `number`: Block number.
    *   `hash`: Block hash.
*   **`P2P_SEND_BLOCK`**: Triggered when a block is broadcasted to a peer.
    *   `peer`: Remote peer ID.
    *   `number`: Block number.
    *   `hash`: Block hash.

### Sync & Downloads

*   **`DOWNLOAD_HEADERS_START`** / **`DOWNLOAD_HEADERS_END`**: Tracking header sync latency and throughput.
*   **`DOWNLOAD_BODIES_START`** / **`DOWNLOAD_BODIES_END`**: Tracking block body download latency.
*   **`DOWNLOAD_POOLED_TXS_END`**: Triggered when pooled transactions (mempool sync) are successfully downloaded.

## Usage for Experiments

These logs can be parsed to calculate various metrics using the provided scripts.

### Running the Evaluation Script

A Python-based evaluation script is available in `scripts/v2/evaluate_logs.py`. You can run it using the wrapper script:

```bash
./scripts/v2/evaluate.sh [path_to_logs_dump]
```

By default, it looks for logs in `./logs_dump`.

### Calculated Metrics

1.  **Inclusion Latency**: Measure the time difference between `P2P_RECV_TX` and the first `TX_EXEC_START` or `BLOCK_EXEC_END` that contains that transaction hash.
2.  **Submit/Commit Ratio**: Compare the count of `P2P_RECV_TX` events with the number of transactions successfully included in blocks over a period of time.
3.  **Propagation Latency**: Compare `P2P_RECV_BLOCK` timestamps across multiple nodes for the same block hash.
4.  **Execution Throughput**: Analyze `BLOCK_EXEC_END` events to see transactions per second (TPS) and gas per second.

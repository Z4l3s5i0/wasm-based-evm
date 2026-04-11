### Ethereum JSON-RPC API Methods Status

This document provides a comprehensive list of Ethereum JSON-RPC methods, categorized by their domain, and indicates whether they have been implemented in the current project refactor.

#### Summary of Implementation
- **Total Standard Methods tracked:** 38
- **Implemented:** 20
- **Pending/Placeholder:** 18

---

### 1. General Node & Network (`EthController`)
These methods provide information about the network status, node configuration, and basic chain data.

| Method | Status | Description |
| :--- | :--- | :--- |
| `eth_chainId` | ✅ Implemented | Returns the chain ID used for signing at the current block. |
| `eth_gasPrice` | ✅ Implemented | Returns the current price per gas in wei (fetched from latest block base fee). |
| `eth_accounts` | ✅ Implemented | Returns a list of addresses owned by client (from genesis/state). |
| `eth_syncing` | ✅ Implemented | Returns an object with data about the sync status or `false`. |
| `eth_mining` | ✅ Implemented | Returns `true` if client is actively mining new blocks. |
| `eth_hashrate` | ❌ Pending | Returns the number of hashes per second that the node is mining with. |
| `eth_protocolVersion` | ❌ Pending | Returns the current ethereum protocol version. |
| `eth_net_version` | ❌ Pending | Returns the current network ID. |

### 2. Account Information (`AccountController`)
Methods related to account state, balances, and code.

| Method | Status | Description |
| :--- | :--- | :--- |
| `eth_getBalance` | ✅ Implemented | Returns the balance of the account of given address. |
| `eth_getTransactionCount` | ✅ Implemented | Returns the number of transactions sent from an address (nonce). |
| `eth_getCode` | ✅ Implemented | Returns code at a given address. |
| `eth_getStorageAt` | ✅ Implemented | Returns the value from a storage position at a given address. |

### 3. Block Data (`BlockController`)
Methods for retrieving block information and chain height.

| Method | Status | Description |
| :--- | :--- | :--- |
| `eth_blockNumber` | ✅ Implemented | Returns the number of most recent block. |
| `eth_getBlockByNumber` | ✅ Implemented | Returns information about a block by number. |
| `eth_getBlockByHash` | ✅ Implemented | Returns information about a block by hash. |
| `eth_getBlockTransactionCountByNumber` | ✅ Implemented | Returns the number of transactions in a block by number. |
| `eth_getBlockTransactionCountByHash` | ✅ Implemented | Returns the number of transactions in a block by hash. |
| `eth_getUncleByBlockHashAndIndex` | ❌ Pending | Returns information about a uncle by block hash and index. |
| `eth_getUncleByBlockNumberAndIndex` | ❌ Pending | Returns information about a uncle by block number and index. |
| `eth_getUncleCountByBlockHash` | ❌ Pending | Returns the number of uncles in a block by hash. |
| `eth_getUncleCountByBlockNumber` | ❌ Pending | Returns the number of uncles in a block by number. |

### 4. Transactions & Receipts (`TransactionController`)
Methods for inspecting specific transactions and their outcomes.

| Method | Status | Description |
| :--- | :--- | :--- |
| `eth_getTransactionByHash` | ✅ Implemented | Returns the information about a transaction requested by hash. |
| `eth_getTransactionReceipt` | ✅ Implemented | Returns the receipt of a transaction by transaction hash. |
| `eth_getTransactionByBlockHashAndIndex` | ❌ Pending | Returns information about a transaction by block hash and index. |
| `eth_getTransactionByBlockNumberAndIndex` | ❌ Pending | Returns information about a transaction by block number and index. |

### 5. Execution & Simulation (`EthController`)
Methods for sending transactions and simulating execution.

| Method | Status | Description |
| :--- | :--- | :--- |
| `eth_sendTransaction` | ✅ Implemented* | Creates new message call transaction or a contract creation (Returns error: needs node signing). |
| `eth_sendRawTransaction` | ❌ Pending | Creates new message call transaction or a contract creation for signed transactions. |
| `eth_call` | ✅ Implemented | Executes a new message call immediately without creating a transaction on the blockchain. |
| `eth_estimateGas` | ✅ Implemented | Generates and returns an estimate of how much gas is necessary to allow the transaction to complete. |

### 6. Event Logs (`LogController`)
Methods for searching and filtering event logs.

| Method | Status | Description |
| :--- | :--- | :--- |
| `eth_getLogs` | ✅ Implemented | Returns an array of all logs matching a given filter object. |
| `eth_newFilter` | ❌ Pending | Creates a filter object, based on filter options, to notify when state changes (logs). |
| `eth_newBlockFilter` | ❌ Pending | Creates a filter in the node, to notify when a new block arrives. |
| `eth_newPendingTransactionFilter` | ❌ Pending | Creates a filter in the node, to notify when new pending transactions arrive. |
| `eth_uninstallFilter` | ❌ Pending | Uninstalls a filter with given id. |
| `eth_getFilterChanges` | ❌ Pending | Polling method for a filter, which returns an array of logs which occurred since last poll. |
| `eth_getFilterLogs` | ❌ Pending | Returns an array of all logs matching filter with given id. |

### 7. Proofs & Miscellaneous
Advanced methods for state verification.

| Method | Status | Description |
| :--- | :--- | :--- |
| `eth_getProof` | ❌ Pending | Returns the account- and storage-values of the specified account including the Merkle-proof. |
| `eth_getWork` | ❌ Pending | Returns the hash of the current block, the seedHash, and the boundary condition to be met. |
| `eth_submitWork` | ❌ Pending | Used for submitting a proof-of-work solution. |
| `eth_submitHashrate` | ❌ Pending | Used for submitting mining hashrate. |

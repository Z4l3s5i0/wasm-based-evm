### Storage Table Responsibilities

This document outlines the responsibilities, use cases, and design rationale for the various tables in the `wasix_eth_storage` crate.

---

### 1. Chain Data
Core blockchain data, indexed primarily by block height or cryptographic hash.

#### `Headers`
- **Responsibility**: Stores the block headers, indexed by block number (`u64`).
- **Used for**: Block verification, light client synchronization, and as the source of block metadata (timestamp, state root, gas limit).
- **Why use it**: Essential for verifying the chain of work and the authenticity of the state.

#### `BlockBodies`
- **Responsibility**: Stores the transactions and ommers associated with a block number.
- **Used for**: Reconstructing full blocks and executing transactions within a block.
- **Why use it**: Separated from headers to allow "header-first" synchronization and to optimize storage for nodes that may prune transaction data.

#### `Transactions`
- **Responsibility**: Stores individual transactions indexed by their transaction hash (`B256`).
- **Used for**: `eth_getTransactionByHash` RPC calls and verifying transaction inclusion.
- **Why use it**: Efficient point lookups for transactions without needing to know which block they are in (once indexed).

#### `Receipts`
- **Responsibility**: Stores transaction receipts (status, gas used, logs) indexed by transaction hash.
- **Used for**: `eth_getTransactionReceipt` RPC calls and log filtering.
- **Why use it**: Post-execution data needed for light client proofs and user feedback on transaction status.

---

### 2. Chain Indexes
Mappings that facilitate fast lookup of chain data.

#### `CanonicalHeads`
- **Responsibility**: Maps block number to the "canonical" block hash.
- **Used for**: Determining which block at a specific height is part of the main chain.
- **Why use it**: Handles chain reorganizations (reorgs). During a reorg, we update this mapping rather than moving data in the `Headers` table.

#### `HeaderNumbers`
- **Responsibility**: Reverse mapping from block hash to block number.
- **Used for**: Finding the height of a block when only its hash is known.
- **Why use it**: Facilitates O(1) lookups for height-based queries.

#### `TransactionLookup`
- **Responsibility**: Maps transaction hash to the block number containing it.
- **Used for**: Locating transaction data and receipts when only the hash is provided.

---

### 3. State Management (Flat State)
The "current" state of the blockchain, optimized for high-performance EVM execution.

#### `Accounts`
- **Responsibility**: Stores the current balance, nonce, and code hash for an Ethereum address.
- **Used for**: EVM execution (checking balances, nonces) and account-related RPCs (`eth_getBalance`).
- **Why use it**: Provides O(1) access to account data by address, which is significantly faster than walking a Merkle Trie.

#### `Storages`
- **Responsibility**: Stores the current values of contract storage slots.
- **Used for**: EVM `SLOAD` operations and `eth_getStorageAt`.
- **Why use it**: Indexed by `(Address, SlotHash)`, allowing direct access to storage values without trie traversal.

#### `Bytecodes`
- **Responsibility**: Stores the actual EVM bytecode, indexed by code hash.
- **Used for**: EVM execution when a contract is called.
- **Why use it**: De-duplicates code across multiple contracts that share the same bytecode (e.g., standard proxy patterns).

---

### 4. Commitment & Proofs (Trie State)
Tables that enable cryptographic commitment (State Root) and Merkle Proofs.

#### `HashedState`
- **Responsibility**: Stores account and storage data indexed by the hash of their address/key.
- **Used for**: Re-calculating the Merkle Patricia Trie (MPT) root.
- **Why use it**: The MPT is keyed by hashes; maintaining a hashed flat state makes trie updates efficient.

#### `TrieNodes`
- **Responsibility**: Stores the nodes of the MPT (Branch, Extension, Leaf).
- **Used for**: Generating the State Root and providing Merkle proofs (`eth_getProof`).
- **Why use it**: Persistent storage of the trie structure avoids re-calculating the entire trie from scratch for every block.

#### `PlainState`
- **Responsibility**: A raw, binary view of account/storage state.
- **Used for**: Fast iteration of state and as a buffer during complex state transitions.

---

### 5. Reorganization Support (ChangeSets)
Historical data used to revert state changes during chain splits.

#### `AccountChangeSets`
- **Responsibility**: Stores the *previous* state of accounts modified in a specific block.
- **Used for**: "Unwinding" state transitions during a reorg.
- **Why use it**: Allows the node to revert to an older state without keeping a full copy of the trie at every block.

#### `StorageChangeSets`
- **Responsibility**: Stores the *previous* values of storage slots modified in a block.
- **Used for**: Reverting contract storage state during reorgs.

---

### 6. Operational & Metadata

#### `Metadata`
- **Responsibility**: Stores key-value pairs for node configuration (e.g., `chain_id`, `genesis_hash`).
- **Used for**: Persistence of node-wide settings.

#### `Forkchoice`
- **Responsibility**: Stores the current head, safe, and finalized block hashes.
- **Used for**: Consensus engine operations and tracking the chain tip.

#### `Payloads`
- **Responsibility**: Temporary storage for block payloads during the Engine API block production process.

#### `ActivePeers`
- **Responsibility**: Tracks current P2P peer information.

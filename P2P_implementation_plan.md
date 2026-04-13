### P2P Architecture Design (JSON-RPC over HTTP)

The P2P stack has been implemented using a robust, JSON-RPC based approach instead of a custom RLPx/libp2p stack. This decision was made to leverage the existing `jsonrpsee` server infrastructure and the mature `reqwest` HTTP client, ensuring better compatibility with WASIX environments.

#### 1. Architectural Layers and Components

The architecture is divided into distinct layers:

*   **Discovery Layer (`discovery`)**
    *   **Responsibility:** Finding other nodes in the network.
    *   **Mechanism:** Recursive polling. Each node periodically calls `p2p_getPeers` on its active peers to find more `SocketAddr`s.
    *   **Integration:** Feeds discovered addresses into the `PeerManager::dial_peer` mechanism.

*   **Peer Management Layer (`peer_manager`)**
    *   **Responsibility:** Managing the lifecycle of peer connections.
    *   **Mechanism:** Centralized in the `PeerManager` struct. It maintains an active pool (`HashMap`) of peers, handles the `MAX_PEERS` limit, and manages the bidirectional `p2p_hello` handshake.
    *   **Health:** Periodically pings all peers; un-responsive peers are removed from the pool.

*   **Transport & RPC Layer (`rpc_client`, `mod.rs`)**
    *   **Responsibility:** Low-level communication and serialization.
    *   **Mechanism:** Uses `jsonrpsee` server for inbound requests and a custom `reqwest`-based `RpcClient` for outbound requests. All "wire" messages are standard JSON-RPC.

*   **Protocol Layer (`P2pApi` trait)**
    *   **Responsibility:** Defining the P2P interface and sub-protocols.
    *   **Methods:** `hello` (handshake), `ping` (health), `get_peers` (discovery), and `gossip` (broadcast).

*   **Service & Sync Layer (Conceptual / Partially Implemented)**
    *   **Responsibility:** Higher-level logic (e.g., Block Sync, Mempool Broadcast).
    *   **Status:** `GossipHandler` is implemented to deserialize Ethereum transactions and feed them into the `Mempool`. Block synchronization remains conceptual.

#### 2. Technical Implementation Details

1.  **Identity:** Each node generates a persistent `secp256k1` keypair (stored in the data directory). The `PeerId` is the hex-encoded public key.
2.  **Handshake:** When `dial_peer` is called, the node sends a `p2p_hello` RPC. The remote node verifies the `PeerId` and initiates a reciprocal `dial_peer` back to the caller's public address if they aren't already bonded.
3.  **Deduplication:** The `PeerManager` ensures that a node is not connected to itself and that multiple connections to the same `PeerId` or `SocketAddr` are prevented.
4.  **Loop Handling:** The reciprocal handshake is carefully controlled to prevent infinite connection loops between two nodes.

#### 3. Conceptually Missing Pieces & Next Steps

Based on the current "JSON-RPC over HTTP" stack, the following components are still missing or require further development:

1.  **Synchronization Engine (Sync Strategy):**
    *   We need a `SyncEngine` that periodically polls peers for their highest block number (`eth_blockNumber`).
    *   Logic to download missing headers and blocks via RPC (e.g., `eth_getBlockByNumber`) and import them into local `storage`.
    *   Deciding between Full Sync (downloading everything) or Snap/Light Sync.

2.  **Mempool Integration (Gossip Wiring):**
    *   **Status:** ✅ **Implemented.**
    *   `GossipHandler` takes the `gossip_rx` channel, deserializes `Vec<u8>` into an Ethereum `TxEnvelope`, and passes valid transactions to the `Mempool` service.
    *   Incoming gossip is automatically re-broadcasted to other peers to ensure network-wide propagation.

3.  **Sophisticated Reputation System:**
    *   The current health check only handles reachability (ping/pong).
    *   We need a scoring system to track "bad" peers (e.g., those sending invalid blocks or spamming gossip) and ban them for a period.

4.  **NAT Traversal & Public IP Discovery:**
    *   The current stack relies on the `--ext-ip` CLI flag.
    *   Automated discovery (e.g., via UPnP or a STUN-like mechanism) would make the node more robust in varied network environments.

5.  **Sub-Protocol Expansion:**
    *   As the EVM matures, we may need specific RPC methods for `snap` (state snapshots) or `wit` (witness data) to support advanced sync modes.
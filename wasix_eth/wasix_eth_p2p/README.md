# `wasix_eth_p2p`

## Overview

* **Purpose:** The `wasix_eth_p2p` crate provides the networking layer for the WASIX Ethereum node. It implements the standard Ethereum peer-to-peer protocols required for node discovery and data exchange.
* **Responsibilities:**
    * **Discovery:** Implementing Node Discovery v4 (UDP-based) to find other nodes in the network.
    * **RLPx:** Implementing the encrypted and authenticated transport protocol (TCP-based) used by Ethereum.
    * **Ethereum Wire Protocol (`eth`):** Handling message exchange for blocks, headers, and transactions.
    * **Peer Management:** Managing the lifecycle of connections, including dialing, handshaking, and session maintenance.
    * **Gossip:** Broadcasting new blocks and transactions to the network.
* **High-level design:** The crate is divided into discovery (UDP/Kademlia) and RLPx (TCP/Encrypted Sessions). A central `PeerManager` orchestrates these components, while a `PeerRegistry` maintains the state of active connections. The `SyncService` acts as a bridge, translating network messages into actions for the node's synchronization and mempool logic.
* **Fit:** It is a core component that sits between the low-level network IO and the high-level synchronization/execution logic.

---

## Module Structure

```text
src/
├── lib.rs              # Crate root and public exports
├── discovery/          # Node Discovery v4 (UDP)
│   ├── mod.rs
│   ├── v4_service.rs   # UDP listener and packet dispatcher
│   ├── kbuckets.rs     # Kademlia routing table
│   └── v4/             # Discovery v4 packet types and handler
├── rlpx/               # RLPx transport and session (TCP)
│   ├── mod.rs
│   ├── server.rs       # TCP listener for inbound connections
│   ├── stream.rs       # Encrypted stream implementation
│   ├── handshake/      # ECIES handshake and protocol negotiation
│   └── session/        # Active peer session management
├── peer/               # Peer management and orchestration
│   ├── mod.rs
│   ├── peer_manager.rs # Main coordinator
│   ├── peer_registry.rs# Active peer session storage
│   ├── dialer.rs       # Outbound connection manager
│   └── sync_service.rs # Bridge to chain sync and mempool
└── error.rs            # P2P specific error types
```

---

## Public API

### `PeerManager` (`peer/peer_manager.rs`)
The main entry point for managing the P2P network.

**Responsibilities:**
- Initializing discovery and RLPx components.
- Providing methods to dial peers or enodes.
- Starting background services.

**Important Methods:**
- `new(...)`: Constructs a new manager with storage and identity.
- `start(...)`: Starts discovery and peer management tasks.
- `dial_peer(addr)`: Initiates a connection to a specific address.

### `P2pServer` (`rlpx/server.rs`)
The TCP server for inbound RLPx connections.

**Responsibilities:**
- Listening for inbound TCP connections.
- Initiating handshakes for new connections.
- Spawning session tasks for successful handshakes.

### `SyncService` (`peer/sync_service.rs`)
The bridge between the network and the node's internal logic.

**Responsibilities:**
- Handling incoming `eth` protocol messages (headers, bodies, receipts).
- Broadcasting new blocks and transactions (Gossip).
- Triggering internal sync processes.

### `DiscoveryV4Service` (`discovery/v4_service.rs`)
The UDP-based node discovery service.

---

## Internal Architecture

* **Major Components:**
    * **Discovery V4:** Uses UDP to maintain a Kademlia-based routing table of known nodes.
    * **RLPx Stream:** A wrapper around `TcpStream` providing ECIES encryption and MAC authentication.
    * **Peer Registry:** A thread-safe container (`Arc<PeerRegistry>`) for all active sessions.
    * **Session Task:** A background task per peer that handles multiplexed message reading and writing.
* **Data Flow:**
    1. **Discovery:** Finds nodes -> Populates `PeerRegistry` / Triggers `PeerDialer`.
    2. **Connection:** `P2pServer` (Inbound) or `PeerDialer` (Outbound).
    3. **Handshake:** Exchange `Hello` and `Status` messages to negotiate capabilities and fork IDs.
    4. **Session:** `SessionTask` handles `eth` protocol messages.
    5. **Processing:** Messages are passed to `SyncService` which interacts with `SyncProvider` and `MempoolProvider`.
* **Ownership Model:** Components are largely shared via `Arc`. The `PeerRegistry` owns `Arc<PeerSession>` objects, which in turn hold handles to background tasks.
* **Async Architecture:** Heavily based on `tokio`. Every connection has its own spawned task. Channels (`mpsc`) are used for inter-component communication (e.g., gossip, disconnect requests).

---

## Dependency Graph

| Dependency | Purpose |
| ---------- | ------- |
| `wasix_eth_types` | Shared Ethereum types and protocol message definitions |
| `wasix_eth_storage` | Persistence for peer data and chain state access |
| `tokio` | Async runtime and networking |
| `alloy-rlp` | RLP encoding for Ethereum wire protocol |
| `secp256k1` | Elliptic curve crypto for handshakes (ECIES) |
| `aes` / `ctr` / `hmac` | Symmetric encryption and auth for RLPx |
| `snap` | Snappy compression for wire messages |

---

## Execution Flow (Outbound Connection)

1. **Discovery:** `DiscoveryManager` selects a node from the routing table or bootnodes.
2. **Dialing:** `PeerDialer` attempts to open a `TcpStream` to the peer.
3. **RLPx Handshake:** Performs ECIES handshake to establish shared secrets.
4. **Protocol Handshake:** Exchange `Hello` (p2p version, caps) and `Status` (network id, fork id).
5. **Registration:** If compatible, the session is added to `PeerRegistry`.
6. **Communication:** `SessionTask` starts handling `eth` messages; `SyncService` is notified of the new peer.

---

## Error Handling

* **P2pError:** Custom enum in `error.rs` covering handshake failures, protocol violations, and IO errors.
* **DisconnectReason:** Standard Ethereum disconnect codes (e.g., `TooManyPeers`, `IncompatibleP2P`).
* **Recoverable Errors:** Connection timeouts or handshake failures result in the peer being temporarily blacklisted or deprioritized.
* **Unrecoverable Errors:** Invalid crypto parameters or critical IO failures during startup.

---

## Configuration

* **Identity:** Local `SecretKey` and generated `PeerId`.
* **Network IDs:** `network_id` and `chain_config` for fork compatibility checks.
* **Ports:** Separate ports for Discovery (UDP) and P2P (TCP).
* **Bootnodes:** List of enode URLs for initial network entry.

---

## Testing

* **Unit Tests:** Found in respective modules (e.g., `kbuckets.rs` tests).
* **Integration Tests:** `tests/` directory (e.g., `handshake_tests.rs`, `sync_service_tests.rs`).
* **Mocking:** Uses `MockP2pSession` and `MockSyncProvider` in tests to isolate networking logic.

---

## Extension Points

* **Capabilities:** The `RlpxStream` supports multiple shared capabilities. New protocols (e.g., `snap`, `light`) can be added by implementing new handlers in the session task.
* **Discovery Versions:** While only V4 is currently fully implemented, the structure allows for V5 integration.

---

## Known Limitations

* **Sequential Dialing:** The dialer currently processes requests with limited parallelism.
* **Discovery V5:** Not implemented; currently relies on V4.
* **Peer Scoring:** A simple peer scoring/reputation system is mentioned in `TODO`s but not fully realized.

---

## Cross-Crate Relationships

* **Depends on:** `wasix_eth_types`, `wasix_eth_storage`, `wasix_eth_utils`.
* **Used by:** `wasix_eth_app` (top-level orchestration) and `wasix_eth_core` (sync triggers).

---

## Developer Notes

* **Gossip Invariants:** Gossip messages are propagated to all active peers except the originator. Ensure `gossip_tx` is set in the `PeerRegistry` for this to function.
* **Handshake Security:** The ECIES handshake is critical for privacy. Do not modify `rlpx/handshake/` without thorough understanding of the Ethereum RLPx specification.
* **Snappy Compression:** Snappy is enabled for `eth` messages version 66 and above. Ensure `wasix_eth_types` message definitions are compatible with RLP encoding/decoding during session tasks.

---

## Summary

`wasix_eth_p2p` is a robust implementation of the Ethereum networking stack. It provides everything from low-level encrypted TCP streams to high-level block gossip and Kademlia discovery. Its modular design separates the concerns of transport encryption, peer discovery, and application-level protocol handling, making it a critical yet maintainable part of the WASIX Ethereum node.
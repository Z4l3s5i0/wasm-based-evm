# `wasix_eth_app`

## Overview

* **Purpose:** The `wasix_eth_app` crate is the top-level application layer for the WASIX Ethereum node. It provides the CLI interface, orchestrates the initialization and lifecycle of all node components, and serves as the main entry point for running the node.
* **Responsibilities:**
    * Parsing command-line arguments.
    * Managing the lifecycle of the node (start, run, stop).
    * Orchestrating the setup of storage, execution, network, and synchronization subsystems.
    * Implementing the Engine API authentication (JWT).
    * Providing block import utilities from RLP files.
* **High-level design:** It uses a Builder pattern (`AppBuilder`) to configure and construct an `App`, which then manages a `Node` instance. The `Node` is composed of several "Payload" structs that encapsulate the dependencies and state of major subsystems.
* **Fit:** This is the binary crate of the `wasix_eth` project. It integrates all other `wasix_eth_*` crates into a functional Ethereum execution client.

---

## Module Structure

```text
src/
├── main.rs            # Application entry point
├── lib.rs             # Library root
├── app.rs             # App and AppBuilder orchestration logic
├── cli.rs             # CLI argument definitions (clap)
├── node.rs            # Node lifecycle and component management
├── jwt.rs             # JWT authentication middleware for RPC
├── import.rs          # RLP block import logic
└── node_components/   # Modular subsystem payloads
    ├── mod.rs
    ├── execution.rs   # EVM execution and engine logic setup
    ├── network.rs     # P2P and discovery setup
    ├── storage.rs     # Database initialization
    └── sync.rs        # Sync controller and gossip bridge setup
```

---

## Public API

### `App` (`app.rs`)
The main application container.

**Responsibilities:**
- Managing the top-level execution flow (run, init, import).
- Holding the `Node` instance.

**Important Methods:**
- `builder()`: Returns a new `AppBuilder`.
- `run()`: Starts the node and waits for it to complete.
- `init()`: Initializes the database with genesis state.
- `import()`: Performs block import from configured sources.

### `AppBuilder` (`app.rs`)
A configuration builder for `App`.

**Responsibilities:**
- Collecting configuration from `Args`.
- Setting up logging, JWT secrets, and RPC servers.
- Constructing the `Node` and its payloads.

**Important Methods:**
- `with_config(args: Args)`: Configures the builder with CLI arguments.
- `build()`: Constructs a full `App` for running the node.
- `build_init()`: Constructs an `App` for database initialization.
- `build_import()`: Constructs an `App` for block importing.

### `Node` (`node.rs`)
The core container for all node subsystems.

**Responsibilities:**
- Holding `Arc` references to all major components (Engine, Sync, Network, Storage).
- Managing background tasks.

**Important Methods:**
- `new()`: Asynchronously initializes all subsystems.
- `start()`: Spawns background tasks for all subsystems.

### `Args` / `Commands` (`cli.rs`)
CLI argument parsing structures using `clap`.

---

## Internal Architecture

* **Major Components:**
    * **Storage:** Uses `wasix_eth_storage` to manage persistent state.
    * **Execution:** Orchestrates `wasix_eth_core` (Engine) and `wasix_eth_execution` (EVM).
    * **Network:** Managed by `wasix_eth_p2p`, including Discovery v4 and P2P gossip.
    * **Sync:** Coordinates block synchronization between the network and the engine.
* **Data Flow:** Transactions enter via RPC or Gossip, are stored in the Mempool, and then processed by the Engine into blocks. Blocks are either received via Sync/Gossip or produced by the Engine.
* **Ownership Model:** Heavily relies on `Arc` for shared ownership of components between different subsystems and background tasks.
* **Async Architecture:** Built on `tokio`. Subsystems spawn background tasks (e.g., gossip loop, sync loop) which are tracked by the `Node` for graceful shutdown.
* **Locking Strategy:** Synchronization is mostly handled within the lower-level crates (e.g., `wasix_eth_core`), while `wasix_eth_app` focuses on wiring them together.

---

## Dependency Graph

| Dependency | Purpose |
| ---------- | ------- |
| `wasix_eth_core` | Core Ethereum logic (Engine API, Mempool, Consensus) |
| `wasix_eth_storage` | Persistent storage (MDBX-based) |
| `wasix_eth_p2p` | Networking layer (Discovery, RLPx, Gossip) |
| `wasix_eth_execution` | EVM execution engine |
| `wasix_eth_types` | Shared Ethereum types |
| `tokio` | Async runtime |
| `clap` | Command-line argument parsing |
| `jsonrpsee` | JSON-RPC server and client implementation |
| `hmac` / `sha2` | JWT authentication |

---

## Execution Flow

1. **CLI Parsing:** `main.rs` parses arguments using `cli.rs`.
2. **Builder Setup:** `AppBuilder` is initialized with configuration.
3. **Component Wiring:** `AppBuilder::build()` calls `Node::new()`.
4. **Subsystem Payloads:** `Node::new()` creates `StoragePayload`, `ExecutionPayload`, `NetworkPayload`, and `SyncPayload` in sequence.
5. **RPC Initialization:** `AppBuilder` starts the HTTP/WS servers for Eth and Engine APIs.
6. **Task Spawning:** `App::run()` calls `Node::start()`, which spawns background tasks for P2P, Sync, and Gossip.
7. **Main Loop:** The application waits for a shutdown signal or for tasks to complete.

---

## Error Handling

* **Custom Errors:** Uses `Box<dyn Error>` for top-level application errors.
* **RPC Errors:** Custom `RpcError` types used in `jwt.rs` for authentication failures.
* **Logging:** Uses `wasix_eth_utils::info!`, `warn!`, and `error!` for diagnostics.
* **Recoverable Errors:** Block import failures are logged but don't necessarily terminate the process.

---

## Configuration

* **CLI Args:** Extensive options for data directories, ports (P2P, RPC, Metrics), bootnodes, and chain configuration.
* **Environment Variables:** Indirectly via `clap`.
* **JWT Secret:** Can be provided via a file (`--auth-rpc-jwt-path`) or auto-generated.
* **Genesis:** Configured via `--genesis-path`.

---

## Testing

* **Unit/Integration Tests:** Located in the `tests/` directory.
    * `app_tests.rs`: Tests high-level application flow.
    * `cli_tests.rs`: Verifies argument parsing.
    * `jwt_tests.rs`: Detailed tests for Engine API authentication.
    * `node_init_tests.rs`: Tests node and database initialization.
    * `import_tests.rs`: Tests RLP block importing.
* **Mocking:** Uses `NoopSync` in some network configurations for isolated testing.

---

## Extension Points

* **Node Components:** New subsystems can be added by creating new payload types in `node_components/`.
* **RPC Modules:** `AppBuilder::setup_rpc_services` is where new RPC methods or modules can be registered.
* **Middleware:** `jwt.rs` provides a template for adding more `tower` or `jsonrpsee` middleware.

---

## Known Limitations

* **Error Propagation:** Some errors in background tasks might only be logged rather than causing a controlled node shutdown.
* **Hardcoded Defaults:** Some configuration values (like max drift in JWT) are currently constants in the code.
* **Import Strategy:** The block import process in `import.rs` is sequential and might be slow for very large chains.

---

## Developer Notes

* **JWT Invariants:** The `JwtAuthService` strictly enforces JWT for any RPC method starting with `engine_`. Ensure tokens are updated periodically as the `iat` claim is validated with a 60-second drift allowance.
* **Shutdown:** The `Node` implements `Drop` to abort background tasks, but `App::run` relies on `tokio::signal` for graceful termination.
* **Storage Pathing:** The `data_dir` and `peer_name` combine to form the actual database path. Changing `peer_name` results in a new, empty database.

---

## Summary

`wasix_eth_app` is the orchestrator of the WASIX Ethereum client. It provides a robust CLI, wires together complex subsystems like the WASM-based EVM, P2P networking, and MDBX storage, and ensures secure communication via JWT-authenticated Engine APIs. Its modular "Payload" architecture allows for clear separation of concerns during the complex initialization phase of an Ethereum node.
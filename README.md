# wasm-based-evm

## Usage

### Install
- Install wasmer
- Install rustup
- Install cargo
- Install wasix
- Install [wasi-sdk](https://github.com/WebAssembly/wasi-sdk/releases)
- Install [protobuf](https://protobuf.dev/installation/) 
- Install [Clang+LLVM](https://releases.llvm.org/)

- Clone https://github.com/rust-ethereum/evm to /evm
- in evm/precompile/src/kzg.rs
import this line 
```rust
use alloc::vec::Vec;
```

### Build and run
#### build target different than wasix
1. Go to [Cargo.toml](./wasix-based-evm/Cargo.toml).
2. Make sure the lines x-y are commented and the lines x-y are uncommented.
3. Follow these shell instructions:
```shell
cd wasix-based-evm
rm .\Cargo.lock   
cargo clean
cargo wasix clean
cargo update
cargo build --release
.\target\release\wasix-based-evm.exe --p2p-port <port> --discovery-port <other-port> --rpc-port <other-other-port> --bootnodes <bootnode-address, other-bootnode-address,...> --max_peers <max-connections> --ext_ip <optional-external-ip> --data_dir <path-to-data-dir> --verbose <_:none, 1:info, 2:debug>
```
#### build target wasix
1. Go to [Cargo.toml](./wasix-based-evm/Cargo.toml).
2. Make sure the lines x-y are commented and the lines x-y are uncommented.
3. Follow these shell instructions:
```shell
cd wasix-based-evm
rm .\Cargo.lock   
cargo clean
cargo wasix clean
cargo update
cargo wasix build --release

wasmer run . --enable-threads --net --http-client --enable-exceptions --volume ./data-dir:./data-dir --enable-bulk-memory --enable-simd --enable-reference-types --enable-multi-value --llvm --enable-async-threads -v \
-- --verbose 2 --data-dir data-dir\
--p2p-port <port> --discovery-port <other-port> --rpc-port <other-other-port> --bootnodes <bootnode-address, other-bootnode-address,...> --max_peers <max-connections> --ext_ip <optional-external-ip>
```

### Monitoring
To monitor the node performance, you can use Prometheus and Grafana.

#### 1. Install Prometheus and Grafana
The easiest way is to use Docker:
```shell
docker run -d --name prometheus -p 9090:9090 prom/prometheus
docker run -d --name grafana -p 3001:3000 grafana/grafana
```

#### 2. Configure Prometheus
Add the following to your `prometheus.yml`:
```yaml
scrape_configs:
  - job_name: 'wasm-evm'
    static_configs:
      - targets: ['localhost:9050'] # Replace with your node's metrics port
```

#### 3. Import Dashboards
Pre-configured dashboards are available in `./wasix-based-evm/grafana/dashboards/`:
- `global-comparison.json`
- `execution-efficiency.json`
- `network-health.json`
- `system-resources.json`

Open Grafana at `http://localhost:3001`, go to **Dashboards** -> **Import**, and upload these JSON files.

### references
for the [Ethereum Json-Rpc specification](https://ethereum.github.io/execution-apis/)
for the [Ethereum Netwrok-layer specification (p2p)](https://ethereum.org/developers/docs/networking-layer/)

for the [patched repos](https://wasix.org/docs/language-guide/rust/patched-repos)
https://hackmd.io/@danielrachi/engine_api

### smart contracts

| Action          | `to`      | `data`        | EVM behavior                         |
|-----------------| --------- | ------------- | ------------------------------------ |
| ETH to user     | recipient | empty         | update balances only                 |
| ETH to contract | contract  | empty         | execute `receive()` / fallback       |
| Contract call   | contract  | function+args | run function logic, may change state |
| Contract deploy | null      | bytecode      | run constructor, store code          |

| Action                   | New block? | State updated? |
| ------------------------ | ---------- | -------------- |
| Deploy contract (tx)     | ✅ Yes      | ✅ Yes          |
| Call contract (tx)       | ✅ Yes      | ✅ Yes          |
| Call contract (eth_call) | ❌ No       | ❌ No           |

### Test the execution client

```shell
cd tester
cargo build
cargo run
```
For example use cases to test, see the Readme [here](./tester/README.md).

### Missing Todos
- [ ] encryption for connections to execution client
- [ ] complete spec compliant p2p
- [ ] implementing persistent storage
- [ ] including features of forks after shanghai-capella
- [ ] metrics dumping
- [ ] State pruning
- [ ] comprehensive syncing
- [ ] browser compatibility
- [ ] Genesis Account setup
- [ ] implementing the [EIP-2718](https://eips.ethereum.org/EIPS/eip-2718) typed transaction signing
- [ ] implementing the [EIP-1559](https://eips.ethereum.org/EIPS/eip-1559) fee schedule
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
.\target\release\wasix-based-evm.exe --p2p-port <port> --discovery-port <other-port> --eth-rpc-port <other-other-port> --bootnodes <bootnode-address, other-bootnode-address,...> --max_peers <max-connections> --ext_ip <optional-external-ip> --data_dir <path-to-data-dir> --verbose <_:none, 1:info, 2:debug>
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
--p2p-port <port> --discovery-port <other-port> --eth-rpc-port <other-other-port> --bootnodes <bootnode-address, other-bootnode-address,...> --max_peers <max-connections> --ext_ip <optional-external-ip>
```

### references
for the [Ethereum Json-Rpc specification](https://ethereum.github.io/execution-apis/)
for the [Ethereum Netwrok-layer specification (p2p)](https://ethereum.org/developers/docs/networking-layer/)
for the [Ethereum Improvement Proposals](https://eips.ethereum.org/)
for the [patched repos](https://wasix.org/docs/language-guide/rust/patched-repos)
https://hackmd.io/@danielrachi/engine_api

## Todos
* [ ] adjust this README
* [ ] do TODOS of README in [scripts](./scripts/README.md)
* [ ] try diablo workloads on client
* [ ] create contender script or workflow to spam diablo workloads
* [ ] adjust experiment proposal
* [x] adjust METRICS exposed via prometheus 
* [ ] adjust dashboards via grafana
* [x] create a storage system for prometheus metrics of different nodes for the experiments for analysis
* [x] adjust the client for passing last few HIVE tests in rust compiled client
  * [x] 226/226 passed for engine-cancun
  * [x] 129/129 passed for engine-api
  * [x] 35/35 passed for engine-withdrawals
  * [x] 8/8 passed for engine-auth
  * [ ] 13/16 passed for discv4
  * [ ] 1/2 passed for sync
  *  --> 412/416
* [ ] provide docker images for the experiments
* [ ] create a docker image for the client
# wasm-based-evm

## Usage

### Install
- Install wasmer
- Install rustup
- Install cargo
- Install wasix
- Install [wasi-sdk](https://github.com/WebAssembly/wasi-sdk/releases)
- Install (protobuf)[https://protobuf.dev/installation/] 


- Clone https://github.com/rust-ethereum/evm to /evm
- in evm/precompile/src/kzg.rs
import this line 
```rust
use alloc::vec::Vec;
```

### Build and run
```shell
cd wasix-based-evm
cargo wasix build --release
wasmer run .\wasm-based-evm\wasix-based-evm\target\wasm32-wasmer-wasi\debug\wasix-based-evm.wasm --net --enable-threads --enable-exceptions --volume ./genesis:./genesis 
```

### references
for the [Ethereum Json-Rpc specification](https://ethereum.github.io/execution-apis/)
for the [Ethereum Netwrok-layer specification (p2p)](https://ethereum.org/developers/docs/networking-layer/)

for the [patched repos](https://wasix.org/docs/language-guide/rust/patched-repos)


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
- [ ] initializing of the execution client with command line arguments
- [ ] complete p2p
- [ ] implementing persistent storage
- [ ] complete engine_api specs
- [ ] including features of forks after shanghai-capella

For an AI analysis of missing components see [here](KI_Analysis_missing_pieces.md) 
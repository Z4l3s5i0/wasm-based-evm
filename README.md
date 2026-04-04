# wasm-based-evm

## Usage

### Install
- Install wasmer
- Install rustup
- Install cargo
- Install wasix
- Install [wasi-sdk](https://github.com/WebAssembly/wasi-sdk/releases)
- Instal nasm with 
```choco
choco install nasm
```

- Clone https://github.com/rust-ethereum/evm to /evm
- in evm/precompile/src/kzg.rs
import this line 
```rust
use alloc::vec::Vec;
```

### Build and run
```shell
cargo wasix build --release
wasmer run target/wasm32-wasmer-wasi/release/wasix-based-evm.wasm --net --enable-threads
```

### references
for the [Ethereum Json-Rpc specification](https://ethereum.github.io/execution-apis/)

for the [patched repos](https://wasix.org/docs/language-guide/rust/patched-repos)


### smart contracts

| Action          | `to`      | `data`        | EVM behavior                         |
|-----------------| --------- | ------------- | ------------------------------------ |
| ETH to user     | recipient | empty         | update balances only                 |
| ETH to contract | contract  | empty         | execute `receive()` / fallback       |
| Contract call   | contract  | function+args | run function logic, may change state |
| Contract deploy | null      | bytecode      | run constructor, store code          |


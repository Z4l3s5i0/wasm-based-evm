# wasm-based-evm

## Usage

### Install
- Install wasmer
- Install rustup
- Install cargo
- Install wasix
- Install [wasi-sdk](https://github.com/WebAssembly/wasi-sdk/releases)
- install (protobuf)[https://protobuf.dev/installation/] 
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
wasmer run .\wasm-based-evm\wasix-based-evm\target\wasm32-wasmer-wasi\release\wasix-based-evm.wasm --net --enable-threads --enable-exceptions   
```

### references
for the [Ethereum Json-Rpc specification](https://ethereum.github.io/execution-apis/)

for the [patched repos](https://wasix.org/docs/language-guide/rust/patched-repos)


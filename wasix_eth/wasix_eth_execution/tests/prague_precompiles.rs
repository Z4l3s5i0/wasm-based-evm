use wasix_eth_execution::precompiles::PraguePrecompileSet;
use evm::standard::{Config, GasometerState, PrecompileSet, State};
use evm::uint::{H160, U256};
use evm::backend::{InMemoryBackend, InMemoryEnvironment, OverlayedBackend};
use evm::interpreter::runtime::{RuntimeState, Context, TransactionContext};
use std::collections::BTreeMap;
use std::rc::Rc;

fn setup_test<'a>(config: &'a Config) -> (State<'a>, InMemoryBackend) {
    let gasometer = GasometerState::new(1000000, false);
    let state = State {
        runtime: RuntimeState {
            context: Context {
                address: H160::default(),
                caller: H160::default(),
                apparent_value: U256::zero(),
            },
            transaction_context: Rc::new(TransactionContext {
                gas_price: U256::zero(),
                origin: H160::default(),
            }),
            retbuf: Vec::new(),
        },
        gasometer,
        config,
    };
    
    let handler = InMemoryBackend {
        environment: InMemoryEnvironment {
            block_hashes: BTreeMap::new(),
            block_number: U256::zero(),
            block_coinbase: H160::default(),
            block_timestamp: U256::zero(),
            block_difficulty: U256::zero(),
            block_randomness: None,
            block_gas_limit: U256::from(1000000u64),
            block_base_fee_per_gas: U256::zero(),
            blob_base_fee_per_gas: U256::zero(),
            blob_versioned_hashes: Vec::new(),
            chain_id: U256::from(1u64),
        },
        state: BTreeMap::new(),
    };

    (state, handler)
}

#[test]
fn test_bls12_g1add_in_prague_set() {
    let precompiles = PraguePrecompileSet::new();
    let config = Config::prague();
    let (mut state, handler) = setup_test(&config);
    let mut overlay = OverlayedBackend::new(&handler, &config.runtime);

    // Address 0x0b (G1ADD)
    let addr = H160([0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0x0b]);
    let input = vec![0u8; 256]; // Identity + Identity = Identity
    
    let res = precompiles.execute(addr, &input, &mut state, &mut overlay);
    assert!(res.is_some());
    let (exit_result, output) = res.unwrap();
    assert!(exit_result.is_ok());
    assert_eq!(output.len(), 128);
    assert!(output.iter().all(|&b| b == 0));
}

#[test]
fn test_bls12_g1mul_in_prague_set() {
    let precompiles = PraguePrecompileSet::new();
    let config = Config::prague();
    let (mut state, handler) = setup_test(&config);
    let mut overlay = OverlayedBackend::new(&handler, &config.runtime);

    // Address 0x0c (G1MUL)
    let addr = H160([0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0x0c]);
    let mut input = vec![0u8; 160]; // Identity point (128 bytes) * scalar 1 (32 bytes)
    input[159] = 1; 
    
    let res = precompiles.execute(addr, &input, &mut state, &mut overlay);
    assert!(res.is_some());
    let (exit_result, output) = res.unwrap();
    assert!(exit_result.is_ok());
    assert_eq!(output.len(), 128);
    assert!(output.iter().all(|&b| b == 0));
}





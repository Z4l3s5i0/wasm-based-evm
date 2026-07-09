use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_types::{Block, Transaction, ConsensusTransaction};
use wasix_eth_types::genesis::GenesisConfiguration;
use alloy_rlp::Decodable;
// use std::path::PathBuf; // Removed unused import
use tempfile::TempDir;
use std::fs;

#[tokio::test]
async fn test_repro_gas_mismatch_isolated() {
    let genesis_json_path = "../testing/genesis.json";
    let chain_rlp_path = "../testing/chain.rlp";

    if !std::path::Path::new(genesis_json_path).exists() || !std::path::Path::new(chain_rlp_path).exists() {
        println!("Skipping test: genesis.json or chain.rlp not found in testing/");
        return;
    }

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test_repro.db");
    let db = EthDatabase::open(&db_path).expect("Failed to open DB");

    // 1. Initialize Genesis
    println!("Initializing genesis from {}", genesis_json_path);
    let genesis_json = fs::read_to_string(genesis_json_path).expect("Failed to read genesis.json");
    let genesis_config: GenesisConfiguration = serde_json::from_str(&genesis_json).expect("Failed to parse genesis.json");
    db.init_genesis(genesis_config).expect("Failed to init genesis");

    let read_provider = DatabaseReadProvider::new(db.inner());
    let write_provider = DatabaseWriteProvider::new(db.inner());
    let execution = EthExecutionProvider::new(read_provider, write_provider);

    // 2. Load and Execute Blocks
    println!("Loading blocks from {}", chain_rlp_path);
    let chain_rlp = fs::read(chain_rlp_path).expect("Failed to read chain.rlp");
    let mut buf = &chain_rlp[..];
    let mut count = 0;

    while !buf.is_empty() {
        match Block::<Transaction>::decode(&mut buf) {
            Ok(block) => {
                let block_num = block.header.number;
                let block_hash = block.header.hash_slow();
                println!("Executing block {} (hash: {})", block_num, block_hash);
                println!("Block Header Gas Used: {}", block.header.gas_used);
                println!("Transactions count: {}", block.body.transactions.len());
                for (i, tx) in block.body.transactions.iter().enumerate() {
                    println!("  TX {}: hash={:?}, to={:?}, gas_limit={}, input_len={}", 
                        i, tx.tx_hash(), tx.to(), tx.gas_limit(), tx.input().len());
                }
                
                // Execute block
                match execution.execute_block(block.clone()) {
                    Ok((final_block, _receipts)) => {
                        println!("Successfully executed block {}: gas_used={}, state_root={:?}", 
                            block_num, final_block.header.gas_used, final_block.header.state_root);
                        
                        // NEW: Run simulation on top of this block
                        println!("Running simulation on block {}...", block_num);
                        match execution.run_simulation_with_state_root(block.body.transactions.clone(), block.clone(), Some(final_block.header.state_root)) {
                            Ok((results, simulated_block)) => {
                                println!("Simulation successful: txs={}, gas_used={}", results.len(), simulated_block.header.gas_used);
                            }
                            Err(e) => {
                                println!("Simulation FAILED: {}", e);
                                panic!("Simulation FAILED: {}", e);
                            }
                        }
                        
                        count += 1;
                    }
                    Err(e) => {
                        println!("Execution failed for block {}: {}", block_num, e);
                        panic!("Execution failed for block {}: {}", block_num, e);
                    }
                }
                
                if count >= 1 {
                    break;
                }
            }
            Err(e) => {
                panic!("Failed to decode block: {}", e);
            }
        }
    }
    
    println!("Executed {} blocks successfully", count);
}

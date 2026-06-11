use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::read_traits::*;
use wasix_eth_types::genesis::GenesisConfiguration;
use wasix_eth_types::*;
use alloy_primitives::{Address, B256, U256, Bytes};
use std::collections::BTreeMap;
use tempfile::NamedTempFile;
use serde_json::from_slice;

fn setup_db() -> EthDatabase {
    let tmp_file = NamedTempFile::new().unwrap();
    EthDatabase::open(tmp_file.path()).unwrap()
}

#[test]
fn test_genesis_cancun() {
    let db = setup_db();
    let genesis_json = r#"{
  "config": {
    "chainId": 3503995874084926,
    "homesteadBlock": 0,
    "eip150Block": 3,
    "eip155Block": 6,
    "eip158Block": 6,
    "byzantiumBlock": 9,
    "constantinopleBlock": 12,
    "petersburgBlock": 15,
    "istanbulBlock": 18,
    "muirGlacierBlock": 21,
    "berlinBlock": 24,
    "londonBlock": 27,
    "arrowGlacierBlock": 30,
    "grayGlacierBlock": 33,
    "mergeNetsplitBlock": 36,
    "shanghaiTime": 390,
    "cancunTime": 420,
    "pragueTime": 450,
    "terminalTotalDifficulty": 4732736,
    "depositContractAddress": "0x0000000000000000000000000000000000000000",
    "ethash": {},
    "blobSchedule": {
      "cancun": {
        "target": 3,
        "max": 6,
        "baseFeeUpdateFraction": 3338477
      },
      "prague": {
        "target": 6,
        "max": 9,
        "baseFeeUpdateFraction": 5007716
      }
    }
  },
  "nonce": "0x0",
  "timestamp": "0x0",
  "extraData": "0x68697665636861696e",
  "gasLimit": "0x23f3e20",
  "difficulty": "0x20000",
  "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
  "coinbase": "0x0000000000000000000000000000000000000000",
  "alloc": {
    "00000961ef480eb55e80d19ad83579a64c007002": {
      "balance": "0x1"
    }
  },
  "number": "0x0",
  "gasUsed": "0x0",
  "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
  "baseFeePerGas": null,
  "excessBlobGas": null,
  "blobGasUsed": null
}"#;

    let genesis: GenesisConfiguration = from_slice(genesis_json.as_bytes()).expect("Failed to parse genesis JSON");
    
    // Set timestamp to Cancun time (420) to trigger Cancun logic if it's time-based
    let mut genesis_cancun = genesis.clone();
    genesis_cancun.timestamp = Some(U256::from(420));
    
    db.init_genesis(genesis_cancun).unwrap();
    
    let reader = DatabaseReadProvider::new(db.inner());
    let header = reader.header(BlockId::Number(0.into())).unwrap().unwrap();
    
    println!("Genesis Header: {:?}", header);
    
    // In Cancun, blob_gas_used and excess_blob_gas should be Some(0)
    assert!(header.blob_gas_used.is_some(), "blob_gas_used should be Some(0) in Cancun");
    assert_eq!(header.blob_gas_used.unwrap(), 0);
    assert!(header.excess_blob_gas.is_some(), "excess_blob_gas should be Some(0) in Cancun");
    assert_eq!(header.excess_blob_gas.unwrap(), 0);
    assert!(header.parent_beacon_block_root.is_some(), "parent_beacon_block_root should be Some(EMPTY) in Cancun");
}

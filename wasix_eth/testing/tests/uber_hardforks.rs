use wasix_eth_app::app::App;
use wasix_eth_app::cli::{Args, Commands};
use wasix_eth_types::*;
use wasix_eth_utils::info;
use clap::Parser;
use tempfile::TempDir;
use alloy_consensus::{TxLegacy, Signed};
use alloy_primitives::{Address, U256, B256, Bytes, Signature, TxKind};
use wasix_eth_storage::read_traits::BlockProvider;
use wasix_eth_storage::write_traits::{BlockWriter, HeaderWriter};
use alloy_rlp::Encodable;

use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::EthDatabase;
use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_types::genesis::GenesisConfiguration;
use std::sync::Arc;

async fn run_uber_test(hardfork_config_str: &str) {
    // 1. Setup genesis
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();
    
    let sender_address = Address::from_slice(&[0xf3, 0x9F, 0xd6, 0xe5, 0x1a, 0xad, 0x88, 0xF6, 0xF4, 0xce, 0x6a, 0xB8, 0x82, 0x72, 0x79, 0xcf, 0xfF, 0xb9, 0x22, 0x66]);
    
    let genesis_json = format!(r#"{{
        "config": {{
            "chainId": 1337,
            "homesteadBlock": 0,
            "eip150Block": 0,
            "eip155Block": 0,
            "eip158Block": 0,
            "byzantiumBlock": 0,
            "constantinopleBlock": 0,
            "petersburgBlock": 0,
            "istanbulBlock": 0,
            "berlinBlock": 0,
            "londonBlock": 0,
            {}
        }},
        "alloc": {{
            "{:?}": {{
                "balance": "0x1000000000000000000000"
            }}
        }},
        "coinbase": "0x0000000000000000000000000000000000000000",
        "difficulty": "0x0",
        "gasLimit": "0x4000000",
        "nonce": "0x0",
        "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "timestamp": "0x0"
    }}"#, hardfork_config_str, sender_address);
    
    let genesis_config: GenesisConfiguration = serde_json::from_str(&genesis_json).unwrap();

    // 2. Init Storage & Execution
    let data_dir_copy = data_dir.clone();
    let db_path = data_dir_copy.join("db");
    let eth_db = EthDatabase::open(&db_path).expect("Failed to open DB");
    eth_db.init_genesis(genesis_config).expect("Failed to init genesis");
    
    let read_provider = Arc::new(DatabaseReadProvider::new(eth_db.inner()));
    let write_provider = Arc::new(DatabaseWriteProvider::new(eth_db.inner()));
    let execution = EthExecutionProvider::new((*read_provider).clone(), (*write_provider).clone());

    // 4. Deploy ContractUber in Block 1
    // Bytecode for ContractUber from uber_repro.rs
    let bytecode_hex = "0x6080604052348015600f57600080fd5b50610c1f8061001f6000396000f3fe608060405234801561001057600080fd5b50600436106100365760003560e01c8063a329e8de1461003b575b600080fd5b610043610045565b005b600060005b6127108110156100ba5760006001828154811061006557634e487b7160e01b600052603260045260246000fd5b9060005260206000200154600183811061008157634e487b7160e01b600052603260045260246000fd5b90600052602060002001546100989190610b42565b60018101906100a79190610b42565b6100b19190610ba4565b905060010161004b565b5050565b600081519050919050565b6000828201905092915050565b60006100e9826100c0565b9050919050565b60006100f9826100d0565b9050919050565b6000610109826100dd565b9050919050565b610119816100f0565b82525050565b60006020820190506101346000830184610110565b92915050565b600060208201905061014f6000830184610125565b92915050565b6000610162826100c0565b9050919050565b600061017282610157565b9050919050565b61018281610169565b82525050565b600060208201905061019d6000830184610179565b92915050565b60006101ac826100c0565b9050919050565b60006101bc826101a1565b9050919050565b6101cc816101b3565b82525050565b60006020820190506101e760008301846101c3565b92915050565b60006101f6826100c0565b9050919050565b6000610206826101ed565b9050919050565b610216816101fd565b82525050565b6000602082019050610231600083018461020d565b92915050565b6000610240826100c0565b9050919050565b600061025082610237565b9050919050565b61026081610247565b82525050565b600060208201905061027b6000830184610257565b92915050565b600061028a826100c0565b9050919050565b600061029a82610281565b9050919050565b6102aa81610291565b82525050565b60006020820190506102c560008301846102a1565b92915050565b60006102d4826100c0565b9050919050565b60006102e4826102cb565b9050919050565b6102f4816102db565b82525050565b600060208201905061030f60008301846102eb565b92915050565b600061031e826100c0565b9050919050565b600061032e82610315565b9050919050565b61033e81610325565b82525050565b60006020820190506103596000830184610335565b92915050565b6000610368826100c0565b9050919050565b60006103788261035f565b9050919050565b6103888161036f565b82525050565b60006020820190506103a3600083018461037f565b92915050565b60006103b2826100c0565b9050919050565b60006103c2826103a9565b9050919050565b6103d2816103b9565b82525050565b60006020820190506103ed60008301846103c9565b92915050565b60006103fc826100c0565b9050919050565b600061040c826103f3565b9050919050565b61041c81610403565b82525050565b60006020820190506104376000830184610413565b92915050565b6000610446826100c0565b9050919050565b60006104568261043d565b9050919050565b6104668161044d565b82525050565b6000602082019050610481600083018461045d565b92915050565b6000610490826100c0565b9050919050565b60006104a082610487565b9050919050565b6104b081610497565b82525050565b60006020820190506104cb60008301846104a7565b92915050565b60006104da826100c0565b9050919050565b60006104ea826104d1565b9050919050565b6104fa816104e1565b82525050565b600060208201905061051560008301846104f1565b92915050565b6000610524826100c0565b9050919050565b60006105348261051b565b9050919050565b6105448161052b565b82525050565b600060208201905061055f600083018461053b565b92915050565b600061056e826100c0565b9050919050565b600061057e82610565565b9050919050565b61058e81610575565b82525050565b60006020820190506105a96000830184610585565b92915050565b60006105b8826100c0565b9050919050565b60006105c8826105af565b9050919050565b6105d8816105bf565b82525050565b60006020820190506105f360008301846105cf565b92915050565b6000610602826100c0565b9050919050565b6000610612826105f9565b9050919050565b61062281610609565b82525050565b600060208201905061063d6000830184610619565b92915050565b600061064c826100c0565b9050919050565b600061065c82610643565b9050919050565b61066c81610653565b82525050565b60006020820190506106876000830184610663565b92915050565b6000610696826100c0565b9050919050565b60006106a68261068d565b9050919050565b6106b68161069d565b82525050565b60006020820190506106d160008301846106ad565b92915050565b60006106e0826100c0565b9050919050565b60006106f0826106d7565b9050919050565b610700816106e7565b82525050565b600060208201905061071b60008301846106f7565b92915050565b600061072a826100c0565b9050919050565b600061073a82610721565b9050919050565b61074a81610731565b82525050565b60006020820190506107656000830184610741565b92915050565b6000610774826100c0565b9050919050565b60006107848261076b565b9050919050565b6107948161077b565b82525050565b60006020820190506107af600083018461078b565b92915050565b60006107be826100c0565b9050919050565b60006107ce826107b5565b9050919050565b6107de816107c5565b82525050565b60006020820190506107f960008301846107d5565b92915050565b6000610808826100c0565b9050919050565b6000610818826107ff565b9050919050565b6108288161080f565b82525050565b6000602082019050610843600083018461081f565b92915050565b6000610852826100c0565b9050919050565b600061086282610849565b9050919050565b61087281610859565b82525050565b600060208201905061088d6000830184610869565b92915050565b600061089c826100c0565b9050919050565b60006108ac82610893565b9050919050565b6108bc816108a3565b82525050565b60006020820190506108d760008301846108b3565b92915050565b60006108e6826100c0565b9050919050565b60006108f6826108dd565b9050919050565b610906816108ed565b82525050565b600060208201905061092160008301846108fd565b92915050565b6000610930826100c0565b9050919050565b600061094082610927565b9050919050565b61095081610937565b82525050565b600060208201905061096b6000830184610947565b92915050565b600061097a826100c0565b9050919050565b600061098a82610971565b9050919050565b61099a81610981565b82525050565b60006020820190506109b56000830184610991565b92915050565b60006109c4826100c0565b9050919050565b60006109d4826109bb565b9050919050565b6109e4816109cb565b82525050565b60006020820190506109ff60008301846109db565b92915050565b6000610a0e826100c0565b9050919050565b6000610a1e82610a05565b9050919050565b610a2e81610a15565b82525050565b6000602082019050610a496000830184610a25565b92915050565b6000610a58826100c0565b9050919050565b6000610a6882610a4f565b9050919050565b610a7881610a5f565b82525050565b6000602082019050610a936000830184610a6f565b92915050565b6000610aa2826100c0565b9050919050565b6000610ab282610a99565b9050919050565b610ac281610aa9565b82525050565b6000602082019050610add6000830184610ab9565b92915050565b6000610aec826100c0565b9050919050565b6000610afc82610ae3565b9050919050565b610b0c81610af3565b82525050565b6000602082019050610b276000830184610b03565b92915050565b6000610b36826100c0565b9050919050565b50818101908201915050565b6000828203905092915050565b6000610b5c826100c0565b9050919050565b6000610b6c82610b53565b9050919050565b6000610b7c826100dd565b9050919050565b610b8c81610b63565b82525050565b6000602082019050610ba76000830184610b83565b92915050565b6000828202905092915050565b6000610bc1826100c0565b9050919050565b6000610bd182610bb8565b9050919050565b6000610be1826100dd565b9050919050565b610bf181610bc8565b82525050565b6000602082019050610c0c6000830184610be8565b92915050565b00fea26469706673582212204c352a9e29a9143c94c9b31d45c50c3d4c6778f6c449176395b87198e3b0825964736f6c634300081c0033";
    let bytecode = Bytes::from(hex::decode(&bytecode_hex[2..]).unwrap());

    let tx1 = TxLegacy {
        chain_id: Some(1337),
        nonce: 0,
        gas_price: 1000000000,
        gas_limit: 10000000,
        to: TxKind::Create,
        value: U256::ZERO,
        input: bytecode,
    };

    let signature = Signature::test_signature();
    let signed_tx1 = Signed::new_unchecked(tx1, signature, B256::ZERO);
    let tx_envelope1 = Transaction::Legacy(signed_tx1);
    
    // Hardcoded address for Signature::test_signature()
    let sender_address = Address::from_slice(&hex::decode("ba6a456c36e5c031d3e11bb93ecd535224ab7526").unwrap());
    info!("Using sender address: {:?}", sender_address);

    let genesis_json = format!(r#"{{
        "config": {{
            "chainId": 1337,
            "homesteadBlock": 0,
            "eip150Block": 0,
            "eip155Block": 0,
            "eip158Block": 0,
            "byzantiumBlock": 0,
            "constantinopleBlock": 0,
            "petersburgBlock": 0,
            "istanbulBlock": 0,
            "berlinBlock": 0,
            "londonBlock": 0,
            {}
        }},
        "alloc": {{
            "{:?}": {{
                "balance": "0x1000000000000000000000"
            }}
        }},
        "coinbase": "0x0000000000000000000000000000000000000000",
        "difficulty": "0x0",
        "gasLimit": "0x4000000",
        "nonce": "0x0",
        "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
        "timestamp": "0x0"
    }}"#, hardfork_config_str, sender_address);
    let genesis_config: GenesisConfiguration = serde_json::from_str(&genesis_json).unwrap();
    eth_db.init_genesis(genesis_config).expect("Failed to init genesis");

    let header1 = Header {
        parent_hash: read_provider.block_hash(0).unwrap().unwrap(),
        ommers_hash: EMPTY_OMMER_ROOT_HASH,
        beneficiary: Address::ZERO,
        state_root: B256::ZERO, // Building mode
        transactions_root: proofs::calculate_transaction_root(&[tx_envelope1.clone()]),
        receipts_root: proofs::calculate_receipt_root::<Receipt>(&[]),
        logs_bloom: Bloom::ZERO,
        difficulty: U256::ZERO,
        number: 1,
        gas_limit: 30_000_000,
        gas_used: 0,
        timestamp: 1,
        extra_data: Bytes::new(),
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(7),
        withdrawals_root: Some(proofs::calculate_withdrawals_root(&[])),
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
        requests_hash: None,
    };

    let block1 = Block {
        header: header1,
        body: BlockBody {
            transactions: vec![tx_envelope1],
            ommers: vec![],
            withdrawals: Some(vec![].into()),
        },
    };

    info!("Executing block 1 (deployment)");
    let (executed_block1, receipts1) = execution.execute_block_with_commit(block1, true).expect("Block 1 execution failed");
    write_provider.insert_block(executed_block1.clone(), receipts1).expect("Failed to insert block 1");
    
    // Calculate contract address
    let sender = executed_block1.body.transactions[0].recover_signer().unwrap();
    let mut rlp_stream = Vec::new();
    let mut list = Vec::new();
    sender.encode(&mut list);
    0u8.encode(&mut list); // nonce 0
    alloy_rlp::Header { list: true, payload_length: list.len() }.encode(&mut rlp_stream);
    rlp_stream.extend(list);
    let hash = keccak256(&rlp_stream);
    let contract_address = Address::from_word(hash);
    info!("Contract deployed at: {:?}", contract_address);

    // 5. Call checkDistance() in Block 2
    let calldata = hex::decode("a329e8de").unwrap(); // checkDistance()
    let tx2 = TxLegacy {
        chain_id: Some(1337),
        nonce: 1,
        gas_price: 1000000000,
        gas_limit: 30000000,
        to: TxKind::Call(contract_address),
        value: U256::ZERO,
        input: Bytes::from(calldata),
    };

    let signed_tx2 = Signed::new_unchecked(tx2, signature, B256::ZERO);
    let tx_envelope2 = Transaction::Legacy(signed_tx2);

    let header2 = Header {
        parent_hash: executed_block1.header.hash_slow(),
        ommers_hash: EMPTY_OMMER_ROOT_HASH,
        beneficiary: Address::ZERO,
        state_root: B256::ZERO, // Building mode
        transactions_root: proofs::calculate_transaction_root(&[tx_envelope2.clone()]),
        receipts_root: proofs::calculate_receipt_root::<Receipt>(&[]),
        logs_bloom: Bloom::ZERO,
        difficulty: U256::ZERO,
        number: 2,
        gas_limit: 60_000_000,
        gas_used: 0, 
        timestamp: 2,
        extra_data: Bytes::new(),
        mix_hash: B256::ZERO,
        nonce: B64::ZERO,
        base_fee_per_gas: Some(7),
        withdrawals_root: Some(proofs::calculate_withdrawals_root(&[])),
        blob_gas_used: None,
        excess_blob_gas: None,
        parent_beacon_block_root: None,
        requests_hash: None,
    };

    let mut block2 = Block {
        header: header2,
        body: BlockBody {
            transactions: vec![tx_envelope2],
            ommers: vec![],
            withdrawals: Some(vec![].into()),
        },
    };

    info!("Executing block 2 (call checkDistance)");
    // block2.header.gas_used = 210000;
    // block2.header.state_root = B256::from_slice(&[0xaa; 32]); // Dummy state root to avoid building mode heuristic
    
    let result = execution.execute_block_with_commit(block2, true);
    
    match result {
        Ok((executed_block, _)) => {
            info!("Block 2 execution successful. Gas used: {}", executed_block.header.gas_used);
            assert!(executed_block.header.gas_used > 0, "Gas used should be greater than 0");
        },
        Err(e) => {
            panic!("Block 2 execution failed: {}", e);
        }
    }
}

#[tokio::test]
async fn test_uber_paris() {
    run_uber_test(r#""mergeNetsplitBlock": 0"#).await;
}

#[tokio::test]
async fn test_uber_shanghai() {
    run_uber_test(r#""mergeNetsplitBlock": 0, "shanghaiTime": 0"#).await;
}

#[tokio::test]
async fn test_uber_cancun() {
    run_uber_test(r#""mergeNetsplitBlock": 0, "shanghaiTime": 0, "cancunTime": 0"#).await;
}

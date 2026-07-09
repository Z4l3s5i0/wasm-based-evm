use wasix_eth_execution::execution_provider::EthExecutionProvider;
use wasix_eth_execution::config::prepare_execution_env;
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::read_traits::BlockProvider;
use wasix_eth_types::*;
use wasix_eth_utils::info;
use tempfile::TempDir;
use alloy_primitives::{U256, FixedBytes, Log, B256};

#[tokio::test]
async fn test_eip6110_deposit_requests() {
    let temp_dir = TempDir::new().unwrap();
    let db = EthDatabase::open(temp_dir.path().join("test_eip6110.db").as_path()).unwrap();
    db.init_tables().unwrap();

    let mut config = wasix_eth_types::genesis::GenesisConfiguration::default();
    config.config.chain_id = 1;
    config.config.terminal_total_difficulty = Some(U256::ZERO);
    config.config.shanghai_time = Some(0);
    config.config.cancun_time = Some(0);
    config.config.prague_time = Some(0); // Prague active from genesis

    db.init_genesis(config.clone()).unwrap();

    let read_provider = DatabaseReadProvider::new(db.inner());
    let write_provider = DatabaseWriteProvider::new(db.inner());

    let eth_exec = EthExecutionProvider::new(read_provider.clone(), write_provider.clone());

    // Let's test the components first.
    let receipt = Receipt::default();

    let receipts = vec![receipt];
    let deposits = eth_exec.collect_deposits(&receipts);
    assert_eq!(deposits.len(), 0);
    
    // Test with a real log
    let pubkey = FixedBytes::<48>::repeat_byte(0x01);
    let withdrawal_credentials = FixedBytes::<32>::repeat_byte(0x02);
    let amount = 32_000_000_000u64;
    let signature = FixedBytes::<96>::repeat_byte(0x03);
    let index = 456u64;

    // Construct ABI encoded log data
    // 5 offsets (32 bytes each)
    let mut data = vec![0u8; 160];
    let mut current_offset = 160;

    // Pubkey (48 bytes)
    U256::from(current_offset).to_be_bytes::<32>().copy_into_slice(&mut data[0..32]);
    data.extend_from_slice(&U256::from(48).to_be_bytes::<32>());
    data.extend_from_slice(pubkey.as_slice());
    data.extend_from_slice(&vec![0u8; 16]); // padding to 32
    current_offset += 32 + 64; // length (32) + data (64 padded)

    // Withdrawal credentials (32 bytes)
    U256::from(current_offset).to_be_bytes::<32>().copy_into_slice(&mut data[32..64]);
    data.extend_from_slice(&U256::from(32).to_be_bytes::<32>());
    data.extend_from_slice(withdrawal_credentials.as_slice());
    current_offset += 32 + 32;

    // Amount (8 bytes)
    U256::from(current_offset).to_be_bytes::<32>().copy_into_slice(&mut data[64..96]);
    data.extend_from_slice(&U256::from(8).to_be_bytes::<32>());
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&vec![0u8; 24]); // padding
    current_offset += 32 + 32;

    // Signature (96 bytes)
    U256::from(current_offset).to_be_bytes::<32>().copy_into_slice(&mut data[96..128]);
    data.extend_from_slice(&U256::from(96).to_be_bytes::<32>());
    data.extend_from_slice(signature.as_slice());
    current_offset += 32 + 96;

    // Index (8 bytes)
    U256::from(current_offset).to_be_bytes::<32>().copy_into_slice(&mut data[128..160]);
    data.extend_from_slice(&U256::from(8).to_be_bytes::<32>());
    data.extend_from_slice(&index.to_le_bytes());
    data.extend_from_slice(&vec![0u8; 24]); // padding

    let log = Log {
        address: DEPOSIT_CONTRACT_ADDRESS,
        data: alloy_primitives::LogData::new_unchecked(
            vec![eip6110_utils::DEPOSIT_EVENT_SIGNATURE],
            data.into()
        ),
    };
    
    let receipt_with_log = Receipt {
        tx_type: 0,
        receipt: ConsensusReceipt {
            status: Eip658Value::Eip658(true),
            cumulative_gas_used: 42000,
            logs: vec![log],
        },
        logs_bloom: alloy_primitives::Bloom::default(),
    };

    let deposits_2 = eth_exec.collect_deposits(&[receipt_with_log.clone()]);
    assert_eq!(deposits_2.len(), 1);
    assert_eq!(deposits_2[0].pubkey, pubkey);
    assert_eq!(deposits_2[0].amount, amount);
    assert_eq!(deposits_2[0].index, index);

    info!("EIP-6110 test: collect_deposits check passed (with log)");

    // Test full header finalization
    let mut block = Block::<Transaction>::default();
    block.header.number = 1;
    block.header.timestamp = 1000;
    block.header.parent_hash = read_provider.block_hash(0).unwrap().unwrap();
    block.header.base_fee_per_gas = Some(1000000000);
    block.header.withdrawals_root = Some(EMPTY_ROOT_HASH);
    block.header.state_root = B256::ZERO; // Building mode

    // Use a random calculated root
    let calculated_root = B256::repeat_byte(0x42);
    
    let (fork, _config, _env) = prepare_execution_env(1, &config.config, &block.header, None);
    assert_eq!(fork, Hardfork::Prague);

    eth_exec.finalize_block_header_with_requests(&mut block, &[receipt_with_log], 42000, calculated_root, fork).expect("Finalization failed");

    assert!(block.header.requests_hash.is_some());
    assert_ne!(block.header.requests_hash.unwrap(), alloy_eips::eip7685::EMPTY_REQUESTS_HASH);
    
    info!("EIP-6110 test: requests_hash calculation passed: {:?}", block.header.requests_hash);
}

trait CopyIntoSlice {
    fn copy_into_slice(&self, slice: &mut [u8]);
}

impl CopyIntoSlice for [u8; 32] {
    fn copy_into_slice(&self, slice: &mut [u8]) {
        slice.copy_from_slice(self);
    }
}

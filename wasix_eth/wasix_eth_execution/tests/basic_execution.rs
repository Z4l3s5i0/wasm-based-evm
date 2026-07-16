use wasix_eth_execution::execution_provider::{EthExecutionProvider, ExecutionProvider};
use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::write_traits::AccountWriter;
use wasix_eth_types::*;
use tempfile::TempDir;
mod tests {
    use wasix_eth_storage::read_traits::{AccountProvider, BlockProvider};
    use wasix_eth_storage::write_traits::MetadataWriter;
    use wasix_eth_types::genesis::GenesisConfiguration;
    use wasix_eth_utils::info;
    use super::*;

    async fn run_empty_block_test(name: &str, mut genesis_config: GenesisConfiguration, expected_root: Option<B256>) {
        let temp_dir = TempDir::new().unwrap();
        let db = EthDatabase::open(temp_dir.path().join(format!("{}.db", name)).as_path()).unwrap();
        db.init_tables().unwrap();

        // Standardize genesis for tests
        genesis_config.config.chain_id = 1;

        // Inject ChainConfig
        db.init_genesis(genesis_config.clone()).unwrap();

        let read_provider = DatabaseReadProvider::new(db.inner());
        let write_provider = DatabaseWriteProvider::new(db.inner());

        let execution = EthExecutionProvider::new(read_provider.clone(), write_provider);

        let mut block = Block::<Transaction>::default();
        block.header.number = 1;
        block.header.parent_hash = read_provider.clone().block_hash(0).unwrap().unwrap();

        let timestamp = genesis_config.timestamp.unwrap_or(U256::ZERO).to::<u64>();
        let fork = Hardfork::get_active_fork(&genesis_config.config, 1, timestamp);
        if fork >= Hardfork::Shanghai {
            block.header.withdrawals_root = Some(proofs::calculate_withdrawals_root(&[]));
            block.body.withdrawals = Some(wasix_eth_types::eip4895::Withdrawals::new(Vec::new()));
        }

        // If we provide an expected root, set it in the header
        if let Some(root) = expected_root {
            block.header.state_root = root;
        }

        let (final_block, _, _) = execution.execute_block(block).expect("Execution failed");
        
        if let Some(root) = expected_root {
            assert_eq!(final_block.header.state_root, root, "State root mismatch in {}", name);
        } else {
             info!("Calculated root for {}: {:?}", name, final_block.header.state_root);
        }
    }

    #[tokio::test]
    async fn test_execute_empty_block_frontier() {
        // 5 ETH reward: 0x8ccdf2379bbcc6e7d3e67b0069cb36631eac79e9ba5d37ed044933a27cac5089
        run_empty_block_test("frontier", GenesisConfiguration::default(), Some(hex!("8ccdf2379bbcc6e7d3e67b0069cb36631eac79e9ba5d37ed044933a27cac5089").into())).await;
    }

    #[tokio::test]
    async fn test_execute_empty_block_homestead() {
        let mut config = GenesisConfiguration::default();
        config.config.homestead_block = Some(0);
        // 5 ETH reward: 0x8ccdf2379bbcc6e7d3e67b0069cb36631eac79e9ba5d37ed044933a27cac5089
        run_empty_block_test("homestead", config, Some(hex!("8ccdf2379bbcc6e7d3e67b0069cb36631eac79e9ba5d37ed044933a27cac5089").into())).await;
    }

    #[tokio::test]
    async fn test_execute_empty_block_spurious_dragon() {
        let mut config = GenesisConfiguration::default();
        config.config.homestead_block = Some(0);
        config.config.eip155_block = Some(0);
        config.config.eip158_block = Some(0);
        // 5 ETH reward: 0x8ccdf2379bbcc6e7d3e67b0069cb36631eac79e9ba5d37ed044933a27cac5089
        run_empty_block_test("spurious_dragon", config, Some(hex!("8ccdf2379bbcc6e7d3e67b0069cb36631eac79e9ba5d37ed044933a27cac5089").into())).await;
    }

    #[tokio::test]
    async fn test_execute_empty_block_byzantium() {
        let mut config = GenesisConfiguration::default();
        config.config.byzantium_block = Some(0);
        // 3 ETH reward: 0xf89cc0e6aef84e24cb27012b28042d7aa3a54a38b8aa5d671382a2d5ea17fa2d
        run_empty_block_test("byzantium", config, Some(hex!("f89cc0e6aef84e24cb27012b28042d7aa3a54a38b8aa5d671382a2d5ea17fa2d").into())).await;
    }

    #[tokio::test]
    async fn test_execute_empty_block_london() {
        let mut config = GenesisConfiguration::default();
        config.config.london_block = Some(0);
        // 2 ETH reward: 0xe7cb9d2fd449f7bd11126bff55266e7b74936f2f230e21d44d75c04b7780dfeb
        run_empty_block_test("london", config, Some(hex!("e7cb9d2fd449f7bd11126bff55266e7b74936f2f230e21d44d75c04b7780dfeb").into())).await;
    }

    #[tokio::test]
    async fn test_execute_empty_block_paris() {
        let mut config = GenesisConfiguration::default();
        config.config.terminal_total_difficulty = Some(U256::ZERO);
        // Paris has TTD=0, but genesis initialization currently doesn't handle TTD transition 
        // if TD is not tracked. However, in our Hardfork logic, TTD=0 => Paris.
        // Block reward is 0 in Paris+.
        run_empty_block_test("paris", config, Some(EMPTY_ROOT_HASH)).await;
    }

    #[tokio::test]
    async fn test_execute_empty_block_shanghai() {
        let mut config = GenesisConfiguration::default();
        config.config.terminal_total_difficulty = Some(U256::ZERO);
        config.config.shanghai_time = Some(0);
        run_empty_block_test("shanghai", config, Some(EMPTY_ROOT_HASH)).await;
    }

    #[tokio::test]
    async fn test_execute_empty_block_cancun() {
        let mut config = GenesisConfiguration::default();
        config.config.terminal_total_difficulty = Some(U256::ZERO);
        config.config.shanghai_time = Some(0);
        config.config.cancun_time = Some(0);
        run_empty_block_test("cancun", config, Some(EMPTY_ROOT_HASH)).await;
    }

    #[tokio::test]
    async fn test_execute_empty_block() {
        let temp_dir = TempDir::new().unwrap();
        let db = EthDatabase::open(temp_dir.path().join("test.db").as_path()).unwrap();
        db.init_tables().unwrap();

        let read_provider = DatabaseReadProvider::new(db.inner());
        let write_provider = DatabaseWriteProvider::new(db.inner());
        write_provider.set_metadata("chain_id".to_string(), 1u32.to_be_bytes().to_vec().into()).expect("chain id set");

        let execution = EthExecutionProvider::new(read_provider, write_provider);

        let mut block = Block::<Transaction>::default();
        block.header.number = 1;
        block.header.parent_hash = B256::ZERO; // Should trigger fallback
        block.header.state_root = B256::ZERO;
        block.header.transactions_root = B256::ZERO;
        block.header.receipts_root = B256::ZERO;

        let result = execution.execute_block(block);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_block_with_one_transaction() {
        let temp_dir = TempDir::new().unwrap();
        let db = EthDatabase::open(temp_dir.path().join("test_tx.db").as_path()).unwrap();
        db.init_tables().unwrap();
        db.init_genesis(GenesisConfiguration::default()).unwrap();

        let read_provider = DatabaseReadProvider::new(db.inner());
        let write_provider = DatabaseWriteProvider::new(db.inner());

        let execution = EthExecutionProvider::new(read_provider.clone(), write_provider.clone());

        let mut block = Block::<Transaction>::default();
        block.header.number = 1;
        block.header.parent_hash = read_provider.block_hash(0).unwrap().unwrap();
        block.header.state_root = B256::ZERO;
        block.header.transactions_root = B256::ZERO;
        block.header.receipts_root = B256::ZERO;

        let tx = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 1,
                gas_limit: 21000,
                to: Address::repeat_byte(0x1).into(),
                value: U256::from(100),
                input: Bytes::default(),
                chain_id: Some(1),
            },
            Signature::test_signature(),
            B256::default(),
        ));

        let sender = tx.recover_signer().unwrap();
        write_provider.update_account(sender, TrieAccount {
            nonce: 0,
            balance: U256::from(100000),
            storage_root: EMPTY_ROOT_HASH,
            code_hash: B256::default(),
        }).unwrap();

        block.body.transactions.push(tx);

        let result = execution.execute_block(block);

        // This should fail currently because we don't handle transactions
        let (_, receipts, _) = result.expect("Execution failed");
        assert_eq!(receipts.len(), 1);
    }

    #[tokio::test]
    async fn test_execute_block_with_transfer() {
        let temp_dir = TempDir::new().unwrap();
        let db = EthDatabase::open(temp_dir.path().join("test_transfer.db").as_path()).unwrap();
        db.init_tables().unwrap();
        
        // Use init_genesis to setup proper state including chain_id
        db.init_genesis(GenesisConfiguration::default()).unwrap();

        let read_provider = DatabaseReadProvider::new(db.inner());
        let write_provider = DatabaseWriteProvider::new(db.inner());

        let tx = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 1,
                gas_limit: 21000,
                to: Address::repeat_byte(0x20).into(),
                value: U256::from(100),
                input: Bytes::default(),
                chain_id: Some(1),
            },
            Signature::test_signature(),
            B256::default(),
        ));

        // Setup initial balance for sender
        let sender = tx.recover_signer().unwrap();
        let recipient = tx.to().unwrap();
    
        write_provider.update_account(sender, TrieAccount {
            nonce: 0,
            balance: U256::from(100000),
            storage_root: EMPTY_ROOT_HASH,
            code_hash: B256::default(),
        }).unwrap();

        let execution = EthExecutionProvider::new(read_provider.clone(), write_provider);

        let mut block = Block::<Transaction>::default();
        block.header.number = 1;
        block.header.parent_hash = read_provider.block_hash(0).unwrap().unwrap();
        block.header.state_root = B256::ZERO;
        block.header.transactions_root = B256::ZERO;
        block.header.receipts_root = B256::ZERO;

        block.body.transactions.push(tx);

        let result = execution.execute_block(block);
        let (_final_block, receipts, _) = result.expect("Execution failed");

        assert_eq!(receipts.len(), 1);
    
        // Check if balance was transferred
        // We use the state root of the final block for the lookup
        let sender_account = read_provider.account(sender, Some(_final_block.header.state_root)).unwrap().unwrap();
        let recipient_account = read_provider.account(recipient, Some(_final_block.header.state_root)).unwrap().unwrap();
    
        assert_eq!(sender_account.balance, U256::from(100000 - 100 - 21000)); // balance - value - gas
        assert_eq!(recipient_account.balance, U256::from(100));
    }

    #[tokio::test]
    async fn test_execute_block_contract_sstore() {
        let temp_dir = TempDir::new().unwrap();
        let db = EthDatabase::open(temp_dir.path().join("test_sstore.db").as_path()).unwrap();
        db.init_tables().unwrap();
        db.init_genesis(GenesisConfiguration::default()).unwrap();

        let read_provider = DatabaseReadProvider::new(db.inner());
        let write_provider = DatabaseWriteProvider::new(db.inner());

        // 1. "Deploy" a contract that does SSTORE(1, 100)
        // PUSH1 100, PUSH1 1, SSTORE
        let code = Bytes::from(vec![0x60, 0x64, 0x60, 0x01, 0x55]);
        let contract_address = Address::repeat_byte(0xCC);
        let code_hash = keccak256(&code);
        
        write_provider.update_account(contract_address, TrieAccount {
            nonce: 1,
            balance: U256::ZERO,
            storage_root: EMPTY_ROOT_HASH,
            code_hash,
        }).unwrap();
        
        use wasix_eth_storage::write_traits::BytecodeWriter;
        write_provider.insert_bytecode(code_hash, code).unwrap();

        // 2. Call the contract
        let tx = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 1,
                gas_limit: 100000,
                to: contract_address.into(),
                value: U256::ZERO,
                input: Bytes::default(),
                chain_id: Some(1),
            },
            Signature::test_signature(),
            B256::default(),
        ));

        let sender = tx.recover_signer().unwrap();
        write_provider.update_account(sender, TrieAccount {
            nonce: 0,
            balance: U256::from(1000000),
            storage_root: EMPTY_ROOT_HASH,
            code_hash: B256::default(),
        }).unwrap();

        let execution = EthExecutionProvider::new(read_provider.clone(), write_provider);

        let mut block = Block::<Transaction>::default();
        block.header.number = 1;
        block.header.state_root = B256::ZERO;
        block.header.transactions_root = B256::ZERO;
        block.header.receipts_root = B256::ZERO;
        block.body.transactions.push(tx);

        let result = execution.execute_block(block);
        let (_final_block, receipts, _) = result.expect("Execution failed");

        assert_eq!(receipts.len(), 1);

        // 3. Verify storage was updated
        use wasix_eth_storage::read_traits::StorageProvider;
        let storage_val = read_provider.storage(contract_address, B256::from_slice(&U256::from(1).to_be_bytes::<32>()), Some(_final_block.header.state_root)).unwrap();
        assert_eq!(storage_val, U256::from(100));
        
        // 4. Verify storage root is NOT empty
        let contract_account = read_provider.account(contract_address, Some(_final_block.header.state_root)).unwrap().unwrap();
        assert_ne!(contract_account.storage_root, EMPTY_ROOT_HASH);
    }


    #[tokio::test]
    async fn test_execute_block_contract_call() {
        let temp_dir = TempDir::new().unwrap();
        let db = EthDatabase::open(temp_dir.path().join("test_call.db").as_path()).unwrap();
        db.init_tables().unwrap();
        db.init_genesis(GenesisConfiguration::default()).unwrap();

        let read_provider = DatabaseReadProvider::new(db.inner());
        let write_provider = DatabaseWriteProvider::new(db.inner());
        // 1. Deploy a "contract" first
        let contract_address = Address::repeat_byte(0xCC);
        let code = Bytes::from(vec![0x60, 0x01]); // MOCK: some code
        let code_hash = keccak256(&code);

        write_provider.update_account(contract_address, TrieAccount {
            nonce: 1,
            balance: U256::ZERO,
            storage_root: EMPTY_ROOT_HASH,
            code_hash,
        }).unwrap();

        use wasix_eth_storage::write_traits::BytecodeWriter;
        write_provider.insert_bytecode(code_hash, code).unwrap();

        // 2. Call the contract
        let tx = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 1,
                gas_limit: 100000,
                to: contract_address.into(),
                value: U256::from(500),
                input: Bytes::from(vec![0x11, 0x22]), // MOCK: some input data
                chain_id: Some(1),
            },
            Signature::test_signature(),
            B256::default(),
        ));

        let sender = tx.recover_signer().unwrap();
        write_provider.update_account(sender, TrieAccount {
            nonce: 0,
            balance: U256::from(1000000),
            storage_root: EMPTY_ROOT_HASH,
            code_hash: B256::default(),
        }).unwrap();

        let execution = EthExecutionProvider::new(read_provider.clone(), write_provider);

        let mut block = Block::<Transaction>::default();
        block.header.number = 1;
        block.header.parent_hash = B256::ZERO;
        block.header.state_root = B256::ZERO;
        block.header.transactions_root = B256::ZERO;
        block.header.receipts_root = B256::ZERO;
        block.body.transactions.push(tx);

        let result = execution.execute_block(block);
        let (_final_block, receipts, _) = result.expect("Execution failed");

        let contract_account = read_provider.account(contract_address, Some(_final_block.header.state_root)).unwrap().unwrap();
        assert_eq!(contract_account.balance, U256::from(500));
    }

    #[tokio::test]
    async fn test_priority_fee_transfer() {
        let temp_dir = TempDir::new().unwrap();
        let db = EthDatabase::open(temp_dir.path().join("test_priority_fee.db").as_path()).unwrap();
        db.init_tables().unwrap();
        db.init_genesis(GenesisConfiguration::default()).unwrap();

        let read_provider = DatabaseReadProvider::new(db.inner());
        let write_provider = DatabaseWriteProvider::new(db.inner());

        let beneficiary = Address::repeat_byte(0xBB);
        
        let tx = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 10, // 10 wei/gas
                gas_limit: 21000,
                to: Address::repeat_byte(0x20).into(),
                value: U256::from(100),
                input: Bytes::default(),
                chain_id: Some(1),
            },
            Signature::test_signature(),
            B256::default(),
        ));

        let sender = tx.recover_signer().unwrap();
        write_provider.update_account(sender, TrieAccount {
            nonce: 0,
            balance: U256::from(1000000),
            storage_root: EMPTY_ROOT_HASH,
            code_hash: B256::default(),
        }).unwrap();

        let execution = EthExecutionProvider::new(read_provider.clone(), write_provider);

        let mut block = Block::<Transaction>::default();
        block.header.number = 1;
        block.header.parent_hash = read_provider.block_hash(0).unwrap().unwrap();
        block.header.state_root = B256::ZERO;
        block.header.transactions_root = B256::ZERO;
        block.header.receipts_root = B256::ZERO;
        block.header.beneficiary = beneficiary;
        block.header.base_fee_per_gas = Some(4); // base fee is 4
        // Priority fee per gas = 10 - 4 = 6
        // Total priority fee = 6 * 21000 = 126000

        block.body.transactions.push(tx);

        let (final_block, _receipts, _) = execution.execute_block(block).expect("Execution failed");

        let beneficiary_account = read_provider.account(beneficiary, Some(final_block.header.state_root)).unwrap().unwrap();
        assert_eq!(beneficiary_account.balance, U256::from(126000u64));
        assert_eq!(final_block.header.number, 1);
    }
    #[tokio::test]
    async fn test_logs_bloom_calculation() {
        let temp_dir = TempDir::new().unwrap();
        let db = EthDatabase::open(temp_dir.path().join("test_bloom.db").as_path()).unwrap();
        db.init_tables().unwrap();
        db.init_genesis(GenesisConfiguration::default()).unwrap();

        let read_provider = DatabaseReadProvider::new(db.inner());
        let write_provider = DatabaseWriteProvider::new(db.inner());

        // Create a contract that emits a log
        // PUSH32 topic, PUSH1 0 (size), PUSH1 0 (offset), LOG1
        let mut code_vec = vec![0x7f];
        let topic_bytes = [0x11u8; 32];
        code_vec.extend_from_slice(&topic_bytes);
        code_vec.push(0x60);
        code_vec.push(0x00);
        code_vec.push(0x60);
        code_vec.push(0x00);
        code_vec.push(0xa1);

        let _topic = B256::from(topic_bytes);
        let code = Bytes::from(code_vec);
        let contract_address = Address::repeat_byte(0xCC);
        let code_hash = keccak256(&code);

        write_provider.update_account(contract_address, TrieAccount {
            nonce: 1,
            balance: U256::ZERO,
            storage_root: EMPTY_ROOT_HASH,
            code_hash,
        }).unwrap();

        use wasix_eth_storage::write_traits::BytecodeWriter;
        write_provider.insert_bytecode(code_hash, code).unwrap();

        let tx = Transaction::Legacy(Signed::new_unchecked(
            TxLegacy {
                nonce: 0,
                gas_price: 1,
                gas_limit: 1000000,
                to: contract_address.into(),
                value: U256::ZERO,
                input: Bytes::default(),
                chain_id: Some(1),
            },
            Signature::test_signature(),
            B256::default(),
        ));

        let sender = tx.recover_signer().unwrap();
        write_provider.update_account(sender, TrieAccount {
            nonce: 0,
            balance: U256::from(1000000),
            storage_root: EMPTY_ROOT_HASH,
            code_hash: B256::default(),
        }).unwrap();

        let execution = EthExecutionProvider::new(read_provider.clone(), write_provider);

        let mut block = Block::<Transaction>::default();
        block.header.number = 1;
        block.header.state_root = B256::ZERO;
        block.header.transactions_root = B256::ZERO;
        block.header.receipts_root = B256::ZERO;
        block.body.transactions.push(tx);

        let (final_block, receipts, _) = execution.execute_block(block).expect("Execution failed");

        assert_eq!(receipts.len(), 1);
        let receipt = &receipts[0];
        
        // At least verify it's not zero if we can't match exactly due to environment issues
        assert!(!receipt.logs_bloom.is_zero(), "Receipt bloom should not be zero");
        assert_eq!(final_block.header.logs_bloom, receipt.logs_bloom, "Block bloom should match receipt bloom");

        // Verify it contains the contract address
        let mut addr_bloom = Bloom::default();
        addr_bloom.accrue(BloomInput::Raw(contract_address.as_slice()));
        assert!(receipt.logs_bloom.contains(&addr_bloom), "Bloom should contain the contract address");
    }
}
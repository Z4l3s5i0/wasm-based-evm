
use wasix_eth_core::{Consensus, EthConsensus};
use wasix_eth_types::{Block, ChainConfig, Transaction, TxEip7702, Signed, B256, Address, U256, Signature};
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::db::EthDatabase;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_eip7702_rejection_before_prague() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.redb");
    let db = EthDatabase::open(&db_path).unwrap();
    db.init_tables().unwrap();
    let read_storage = Arc::new(DatabaseReadProvider::new(Arc::new(db).inner()));
    let consensus = EthConsensus::new(read_storage.clone());

    // 1. Create a block with EIP-7702 transaction
    let mut block: Block<Transaction> = Block::default();
    block.header.number = 10;
    block.header.timestamp = 1000;
    
    let tx = Transaction::Eip7702(Signed::new_unchecked(
        TxEip7702 {
            chain_id: 1,
            nonce: 0,
            gas_limit: 21000,
            max_fee_per_gas: 100,
            max_priority_fee_per_gas: 10,
            to: Address::ZERO,
            value: U256::from(0),
            access_list: Default::default(),
            authorization_list: vec![],
            input: Default::default(),
        },
        Signature::test_signature(),
        B256::ZERO,
    ));
    
    block.body.transactions = vec![tx];
    
    // Calculate roots to pass basic validation
    block.header.transactions_root = wasix_eth_types::proofs::calculate_transaction_root(&block.body.transactions);
    block.header.ommers_hash = wasix_eth_types::keccak256(alloy_rlp::encode(&block.body.ommers));

    // 2. Setup config where Prague is NOT active at block 10, timestamp 1000
    let mut chain_config = ChainConfig::default();
    chain_config.prague_time = Some(2000); // Prague starts at timestamp 2000

    // 3. Validate body - should fail
    let result = consensus.validate_body(&block, &chain_config);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("EIP-7702 transaction present before Prague activation"));

    // 4. Setup config where Prague IS active
    chain_config.prague_time = Some(500); // Prague starts at timestamp 500

    // 5. Validate body - should pass (EIP-7702 check wise)
    let result = consensus.validate_body(&block, &chain_config);
    assert!(result.is_ok(), "Expected OK, got: {:?}", result.err());
}

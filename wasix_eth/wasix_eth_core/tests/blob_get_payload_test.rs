use wasix_eth_core::engine::Engine;
use wasix_eth_storage::EthDatabase;
use wasix_eth_types::*;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_core::mempool::mempool::Mempool;
use tempfile::tempdir;
use wasix_eth_core::account_manager::AccountManager;
use wasix_eth_core::{ChainManagerImpl, EthConsensus};
use wasix_eth_execution::execution_provider::EthExecutionProvider;
use std::sync::Arc;
use tokio::sync::broadcast;
use wasix_eth_storage::write_traits::{MetadataWriter, BlockWriter};
use wasix_eth_core::mempool::mempool_provider::MempoolProvider;

async fn setup_engine() -> (Engine, Arc<EthDatabase>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_blobs.redb");
    let db = EthDatabase::open(&db_path).unwrap();
    db.init_tables().unwrap();
    
    let db_arc = Arc::new(db);
    let read_storage = DatabaseReadProvider::new(db_arc.inner());
    let write_storage = DatabaseWriteProvider::new(db_arc.inner());
    
    write_storage.set_metadata("chain_id".to_string(), 1u64.to_be_bytes().to_vec().into()).unwrap();

    let execution = Arc::new(EthExecutionProvider::new(read_storage.clone(), write_storage.clone()));
    let account_manager = Arc::new(AccountManager::new_with_dev_keys());
    let (event_tx, _) = broadcast::channel(100);
    let mempool = Arc::new(Mempool::new(U256::ZERO));
    let chain = Arc::new(ChainManagerImpl::new(read_storage.clone(), write_storage.clone()));
    let consensus = Arc::new(EthConsensus::new(Arc::new(read_storage.clone())));
    
    (Engine::new(read_storage, write_storage, execution, account_manager, chain, mempool, event_tx, consensus), db_arc, dir)
}

#[tokio::test]
async fn test_engine_get_payload_v3_blobs() {
    let (engine, _db, _dir) = setup_engine().await;
    
    let blob = Blob::repeat_byte(0xab);
    let commitment = Bytes48::repeat_byte(0xcc);
    let proof = Bytes48::repeat_byte(0xdd);
    let versioned_hash = B256::repeat_byte(0x11);
    
    engine.mempool.add_blob(versioned_hash, blob.clone(), commitment, proof).await;
    
    let mut tx_inner = TxEip4844::default();
    tx_inner.blob_versioned_hashes = vec![versioned_hash];
    let signed_tx = Transaction::Eip4844(tx_inner.into_signed(Signature::test_signature()).into());
    
    let mut block = Block::<Transaction>::default();
    block.body.transactions.push(signed_tx);
    block.header.timestamp = 0x1234;
    
    let payload_id = PayloadId::new([1; 8]);
    let bundle = BlobsBundleV1 {
        blobs: vec![blob.clone()],
        commitments: vec![commitment],
        proofs: vec![proof],
    };
    engine.write_storage.add_payload(payload_id, block, vec![], bundle).unwrap();
    
    let response = engine.get_payload_v3(payload_id).await.unwrap();
    
    assert_eq!(response.blobs_bundle.blobs.len(), 1);
    assert_eq!(response.blobs_bundle.blobs[0], blob);
    assert_eq!(response.blobs_bundle.commitments[0], commitment);
    assert_eq!(response.blobs_bundle.proofs[0], proof);
}

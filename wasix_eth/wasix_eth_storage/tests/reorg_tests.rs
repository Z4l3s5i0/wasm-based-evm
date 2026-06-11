use wasix_eth_storage::EthDatabase;
use wasix_eth_storage::read::DatabaseReadProvider;
use wasix_eth_storage::write::DatabaseWriteProvider;
use wasix_eth_storage::read_traits::*;
use wasix_eth_storage::write_traits::*;
use wasix_eth_types::{Transaction, Signed, TxLegacy, Receipt, ConsensusReceipt, Eip658Value, Header, BlockBody};
use alloy_primitives::{B256, Signature};
use tempfile::NamedTempFile;

fn setup_db() -> EthDatabase {
    let tmp_file = NamedTempFile::new().unwrap();
    EthDatabase::open(tmp_file.path()).unwrap()
}

#[test]
fn test_transaction_receipt_reorg() {
    let db = setup_db();
    let reader = DatabaseReadProvider::new(db.inner());
    let writer = DatabaseWriteProvider::new(db.inner());

    let tx_hash = B256::repeat_byte(0x01);
    let tx = Transaction::Legacy(Signed::new_unchecked(TxLegacy {
        chain_id: Some(1),
        nonce: 1,
        ..Default::default()
    }, Signature::test_signature(), tx_hash));

    // Block A (Side chain)
    let block_a_hash = B256::repeat_byte(0x0a);
    let receipt_a = Receipt {
        receipt: ConsensusReceipt {
            status: Eip658Value::Eip658(true),
            cumulative_gas_used: 1000,
            logs: vec![],
        },
        ..Default::default()
    };

    // Block B (Canonical chain)
    let block_b_hash = B256::repeat_byte(0x0b);
    let receipt_b = Receipt {
        receipt: ConsensusReceipt {
            status: Eip658Value::Eip658(true),
            cumulative_gas_used: 2000, // Different gas used
            logs: vec![],
        },
        ..Default::default()
    };

    // 1. Import Block B (canonical)
    writer.insert_header(1, Header { number: 1, ..Default::default() }).unwrap();
    writer.insert_block_hash(block_b_hash, 1).unwrap();
    writer.insert_block_body(block_b_hash, 1, BlockBody { transactions: vec![tx.clone()], ..Default::default() }).unwrap();
    writer.insert_receipt(block_b_hash, 0, receipt_b.clone()).unwrap();
    writer.insert_transaction_lookup(tx_hash, block_b_hash, 0).unwrap();
    writer.set_canonical(1, block_b_hash).unwrap();
    writer.update_forkchoice(block_b_hash, None, None).unwrap();

    // Verify it works
    let r = reader.transaction_receipt(tx_hash).unwrap().unwrap();
    assert_eq!(r.receipt.cumulative_gas_used, 2000);

    // 2. Import Block A (side chain)
    // This will overwrite the lookup entry!
    writer.insert_header(1, Header { number: 1, extra_data: vec![1].into(), ..Default::default() }).unwrap();
    writer.insert_block_hash(block_a_hash, 1).unwrap();
    writer.insert_block_body(block_a_hash, 1, BlockBody { transactions: vec![tx.clone()], ..Default::default() }).unwrap();
    writer.insert_receipt(block_a_hash, 0, receipt_a.clone()).unwrap();
    writer.insert_transaction_lookup(tx_hash, block_a_hash, 0).unwrap();

    // NOW: Lookup[tx_hash] points to Block A.
    // BUT Block B is still canonical.
    assert_eq!(reader.block_hash(1).unwrap(), Some(block_b_hash));

    // BEFORE FIX: transaction_receipt(tx_hash) would return None because it looks up A,
    // and A is not canonical.
    // AFTER FIX: It should still work IF we either store multiple lookups OR update lookup on reorg.
    // Wait, in this case we didn't reorg yet, B is still head.
    
    // If I use my new implementation:
    // transaction_receipt(tx_hash) -> lookup points to A -> A is not canonical -> return None.
    let r_opt = reader.transaction_receipt(tx_hash).unwrap();
    assert!(r_opt.is_none());
    
    // 3. Reorg to Block B (explicitly again)
    // This should update the lookup!
    // In Engine, forkchoiceUpdated calls update_transaction_lookup.
    // Here we simulate it.
    
    // Simulation of Engine::forkchoice_updated logic:
    writer.insert_transaction_lookup(tx_hash, block_b_hash, 0).unwrap();
    
    let r2 = reader.transaction_receipt(tx_hash).unwrap().unwrap();
    assert_eq!(r2.receipt.cumulative_gas_used, 2000);

    // 4. Move head to Block A
    writer.set_canonical(1, block_a_hash).unwrap();
    writer.update_forkchoice(block_a_hash, None, None).unwrap();
    // Simulate Engine updating lookup on reorg
    writer.insert_transaction_lookup(tx_hash, block_a_hash, 0).unwrap();

    let r3 = reader.transaction_receipt(tx_hash).unwrap().unwrap();
    assert_eq!(r3.receipt.cumulative_gas_used, 1000);
}

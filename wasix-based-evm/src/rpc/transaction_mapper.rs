use alloy_consensus::{TxEnvelope, ReceiptWithBloom as ConsensusReceipt, transaction::SignerRecoverable as _, transaction::Recovered};
use alloy_primitives::{B256, Address};
use alloy_rpc_types::{Transaction, TransactionReceipt};

pub struct TransactionMapper;

impl TransactionMapper {
    pub fn to_rpc_transaction(tx: TxEnvelope, block_ref: Option<(u64, B256, usize)>) -> Transaction {
        let (block_number, block_hash, transaction_index) = match block_ref {
            Some((num, hash, index)) => (Some(num), Some(hash), Some(index as u64)),
            None => (None, None, None),
        };

        let from = tx.recover_signer().ok().unwrap_or_default();
        let inner = Recovered::new_unchecked(tx, from);

        Transaction {
            inner,
            block_hash,
            block_number,
            transaction_index,
            effective_gas_price: None,
        }
    }

    pub fn to_rpc_receipt(receipt: ConsensusReceipt, block_ref: Option<(u64, B256, usize)>) -> TransactionReceipt {
        let (block_number, block_hash, transaction_index) = match block_ref {
            Some((num, hash, index)) => (Some(num), Some(hash), Some(index as u64)),
            None => (None, None, None),
        };

        let rpc_logs: Vec<alloy_rpc_types::Log> = receipt.receipt.logs.iter().map(|l| alloy_rpc_types::Log {
            inner: l.clone(),
            block_number,
            block_hash,
            transaction_hash: None,
            transaction_index,
            log_index: None,
            removed: false,
            block_timestamp: None,
        }).collect();

        let rpc_receipt_with_bloom = alloy_consensus::ReceiptWithBloom {
            receipt: alloy_consensus::Receipt {
                status: receipt.receipt.status,
                cumulative_gas_used: receipt.receipt.cumulative_gas_used,
                logs: rpc_logs,
            },
            logs_bloom: receipt.logs_bloom,
        };

        TransactionReceipt {
            inner: alloy_rpc_types::ReceiptEnvelope::Legacy(rpc_receipt_with_bloom),
            transaction_hash: B256::ZERO, 
            transaction_index,
            block_hash,
            block_number,
            from: Address::ZERO,
            to: None,
            effective_gas_price: 0,
            gas_used: 0,
            contract_address: None,
            blob_gas_used: None,
            blob_gas_price: None,
        }
    }
}

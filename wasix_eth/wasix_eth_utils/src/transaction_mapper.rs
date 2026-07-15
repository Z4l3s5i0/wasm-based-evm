use wasix_eth_types::{Transaction, Address, RpcTransaction, RpcTransactionReceipt, B256, Receipt as ConsensusReceipt, ReceiptWithBloom, SignerRecoverable, ConsensusTransaction, Header};
use wasix_eth_types::error::RpcError;

pub struct TransactionMapper;

impl TransactionMapper {
    pub fn to_rpc_transaction(tx: Transaction, block_ref: Option<(u64, B256, u64)>, header: Option<Header>) -> RpcTransaction {
        let (block_number, block_hash, transaction_index) = match block_ref {
            Some((num, hash, index)) => (Some(num), Some(hash), Some(index)),
            None => (None, None, None),
        };

        let block_timestamp = header.map(|h| h.timestamp);
        let signer = tx.recover_signer().ok().unwrap_or_default();

        RpcTransaction {
            inner: tx,
            from: signer,
            block_hash,
            block_number,
            transaction_index,
            effective_gas_price: None,
            block_timestamp,
        }
    }

    pub fn to_rpc_receipt(receipt: ConsensusReceipt, meta: Option<wasix_eth_types::ReceiptMeta>, block_ref: Option<(u64, B256, u64)>, transaction: Option<&Transaction>, tx_hash_override: Option<B256>, gas_used: u64, base_fee: Option<u64>, blob_gas_price: Option<u128>) -> RpcTransactionReceipt {
        let (block_number, block_hash, transaction_index) = match block_ref {
            Some((num, hash, index)) => (Some(num), Some(hash), Some(index)),
            None => (None, None, None),
        };

        let transaction_hash = tx_hash_override.or(transaction.map(|tx| *tx.hash())).unwrap_or_default();

        let from = transaction.map(|tx| tx.recover_signer().ok().unwrap_or_default()).unwrap_or_default();
        let to = transaction.and_then(|tx| match tx.kind() {
            wasix_eth_types::TxKind::Call(to) => Some(to),
            wasix_eth_types::TxKind::Create => None,
        });

        let rpc_logs: Vec<alloy_rpc_types::eth::Log> = receipt.receipt.logs.iter().enumerate().map(|(i, l)| {
            let mut log = alloy_rpc_types::eth::Log {
                inner: l.clone(),
                block_number,
                block_hash,
                transaction_hash: Some(transaction_hash),
                transaction_index,
                log_index: Some(i as u64), // Simplified, should be index in block
                removed: false,
                block_timestamp: None,
            };
            log.transaction_hash = Some(transaction_hash);
            log
        }).collect();

        let rpc_receipt = alloy_rpc_types::eth::Receipt {
            status: receipt.receipt.status,
            cumulative_gas_used: receipt.receipt.cumulative_gas_used as u64,
            logs: rpc_logs,
        };

        let inner = match transaction {
            Some(tx) => match tx {
                Transaction::Legacy(_) => alloy_rpc_types::eth::ReceiptEnvelope::Legacy(ReceiptWithBloom {
                    receipt: rpc_receipt,
                    logs_bloom: receipt.logs_bloom,
                }),
                Transaction::Eip2930(_) => alloy_rpc_types::eth::ReceiptEnvelope::Eip2930(ReceiptWithBloom {
                    receipt: rpc_receipt,
                    logs_bloom: receipt.logs_bloom,
                }),
                Transaction::Eip1559(_) => alloy_rpc_types::eth::ReceiptEnvelope::Eip1559(ReceiptWithBloom {
                    receipt: rpc_receipt,
                    logs_bloom: receipt.logs_bloom,
                }),
                Transaction::Eip4844(_) => alloy_rpc_types::eth::ReceiptEnvelope::Eip4844(ReceiptWithBloom {
                    receipt: rpc_receipt,
                    logs_bloom: receipt.logs_bloom,
                }),
                _ => alloy_rpc_types::eth::ReceiptEnvelope::Legacy(ReceiptWithBloom {
                    receipt: rpc_receipt,
                    logs_bloom: receipt.logs_bloom,
                }),
            },
            None => alloy_rpc_types::eth::ReceiptEnvelope::Legacy(ReceiptWithBloom {
                receipt: rpc_receipt,
                logs_bloom: receipt.logs_bloom,
            }),
        };

        let (b_gas_used, b_gas_price) = if let Some(Transaction::Eip4844(signed_tx)) = transaction {
            let hashes = signed_tx.tx().blob_versioned_hashes().unwrap_or_default();
            let used = (hashes.len() as u64) * 131072;
            (Some(used), blob_gas_price)
        } else {
            (None, None)
        };

        let mut rpc_receipt_res = RpcTransactionReceipt {
            inner,
            transaction_hash,
            transaction_index: transaction_index.map(|i| i),
            block_hash,
            block_number,
            from,
            to,
            effective_gas_price: transaction.map(|tx| {
                let max_fee = tx.max_fee_per_gas();
                if let Some(base_fee) = base_fee {
                    let priority_fee = tx.max_priority_fee_per_gas().unwrap_or(max_fee);
                    (base_fee as u128 + std::cmp::min(priority_fee, max_fee.saturating_sub(base_fee as u128))) as u128
                } else {
                    max_fee
                }
            }).unwrap_or(0),
            gas_used,
            contract_address: meta.and_then(|m| m.contract_address),
            blob_gas_used: b_gas_used,
            blob_gas_price: b_gas_price,
        };

        rpc_receipt_res.transaction_hash = transaction_hash;
        if let Some(index) = transaction_index {
            rpc_receipt_res.transaction_index = Some(index);
        }
        if let Some(hash) = block_hash {
            rpc_receipt_res.block_hash = Some(hash);
        }
        if let Some(number) = block_number {
            rpc_receipt_res.block_number = Some(number);
        }
        rpc_receipt_res
    }
    pub fn parse_address(addr: &str) -> Result<Address, RpcError> {
        addr.parse().map_err(|e| RpcError::InvalidParams(format!("Invalid address: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasix_eth_types::{Receipt, ConsensusReceipt, Eip658Value, B256, Bloom};

    #[test]
    fn test_to_rpc_receipt_hash_preservation() {
        let tx_hash = B256::from([0x42; 32]);
        let receipt = Receipt {
            tx_type: 0,
            receipt: ConsensusReceipt {
                status: Eip658Value::Eip658(true),
                cumulative_gas_used: 1000,
                logs: vec![],
            },
            logs_bloom: Bloom::ZERO,
        };
        
        let rpc_receipt = TransactionMapper::to_rpc_receipt(receipt, None, None, None, Some(tx_hash), 1000, None, None);
        
        assert_eq!(rpc_receipt.transaction_hash, tx_hash, "Transaction hash must be preserved in RpcTransactionReceipt");
    }
}

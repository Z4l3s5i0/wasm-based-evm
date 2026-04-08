use tonic::Status;
use crate::rpc::provider_error::ProviderError;
use crate::rpc::evm_rpc::{BlockResponse, LogEntry, TransactionReceiptResponse, TransactionInfoResponse};
use crate::storage::types::{Block, Receipt, Transaction};

pub fn status_from(err: ProviderError) -> Status {
    match err {
        ProviderError::NotFound(msg) => Status::not_found(msg),
        ProviderError::InvalidInput(msg) => Status::invalid_argument(msg),
        ProviderError::Execution(msg) => Status::failed_precondition(msg),
        ProviderError::Internal(msg) => Status::internal(msg),
    }
}

pub fn map_block_response(block: &Block) -> BlockResponse {
    BlockResponse {
        number: block.body.execution_payload.block_number,
        hash: format!("{:?}", block.body.execution_payload.block_hash),
        parent_hash: format!("{:?}", block.body.execution_payload.parent_hash),
        timestamp: block.body.execution_payload.timestamp,
        transactions: block.body.execution_payload.transactions.iter().map(|t| format!("{:?}", t.hash)).collect(),
    }
}

pub fn map_tx_info_response(tx: &Transaction, block: Option<&Block>) -> TransactionInfoResponse {
    TransactionInfoResponse {
        hash: format!("{:?}", tx.hash),
        from: format!("{:?}", tx.from),
        to: tx.to.map(|a| format!("{:?}", a)).unwrap_or_default(),
        value: tx.value.to_string(),
        nonce: tx.nonce,
        data: tx.data.clone(),
        block_number: block.map(|b| b.body.execution_payload.block_number).unwrap_or_default(),
        block_hash: block.map(|b| format!("{:?}", b.body.execution_payload.block_hash)).unwrap_or_default(),
        gas_limit: tx.gas_limit,
        gas_price: tx.gas_price.to::<u64>(),
    }
}

pub fn map_receipt_response(tx: &Transaction, receipt: &Receipt, block: &Block) -> TransactionReceiptResponse {
    let logs = receipt.logs.iter().enumerate().map(|(i, l)| {
        LogEntry {
            address: format!("{:?}", l.address),
            topics: l.topics.iter().map(|t| format!("{:?}", t)).collect(),
            data: l.data.clone(),
            block_number: block.body.execution_payload.block_number,
            block_hash: format!("{:?}", block.body.execution_payload.block_hash),
            transaction_hash: format!("{:?}", tx.hash),
            transaction_index: block.body.execution_payload.transactions.iter().position(|t| t.hash == tx.hash).unwrap_or(0) as u64,
            log_index: i as u64,
        }
    }).collect();

    TransactionReceiptResponse {
        transaction_hash: format!("{:?}", tx.hash),
        transaction_index: block.body.execution_payload.transactions.iter().position(|t| t.hash == tx.hash).unwrap_or(0) as u64,
        block_hash: format!("{:?}", block.body.execution_payload.block_hash),
        block_number: block.body.execution_payload.block_number,
        from: format!("{:?}", tx.from),
        to: tx.to.map(|a| format!("{:?}", a)).unwrap_or_default(),
        cumulative_gas_used: receipt.cumulative_gas_used,
        gas_used: receipt.cumulative_gas_used, // Simplified
        contract_address: String::new(), // TODO: implement if needed
        logs,
        logs_bloom: format!("{:?}", receipt.logs_bloom),
        status: if receipt.success { 1 } else { 0 },
    }
}

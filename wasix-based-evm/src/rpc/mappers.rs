use alloy_primitives::{hex, Address, B256, U256};
use tonic::Status;
use crate::rpc::provider_api::{
    DomainTransactionRequest, DomainProposeBlockRequest, DomainPayloadStatus, 
    DomainPayloadAttributes, DomainForkchoiceUpdatedRequest, DomainForkchoiceUpdatedResponse, 
    DomainGetPayloadRequest, DomainPeerInfo, DomainNetAddPeerRequest, 
    DomainNetNodeInfoResponse
};
use crate::rpc::provider_error::ProviderError;
use crate::rpc::evm_rpc::{
    BlockResponse, LogEntry, TransactionReceiptResponse, TransactionInfoResponse,
    TransactionRequest, ProposeBlockRequest, PayloadStatus as ProtoPayloadStatus,
    ForkchoiceUpdatedRequest as ProtoForkchoiceUpdatedRequest, 
    ForkchoiceUpdatedResponse as ProtoForkchoiceUpdatedResponse,
    GetPayloadRequest as ProtoGetPayloadRequest,
    ExecutionPayload as ProtoExecutionPayload,
    PeerInfo as ProtoPeerInfo, 
    NetAddPeerRequest as ProtoNetAddPeerRequest,
    NetNodeInfoResponse as ProtoNetNodeInfoResponse
};
use crate::storage::types::{Block, Receipt, Transaction, ExecutionPayload as DomainExecutionPayload};

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

pub fn map_transaction_request(req: TransactionRequest) -> Result<DomainTransactionRequest, Status> {
    let from: Address = req.from.parse().map_err(|_| Status::invalid_argument("Invalid from address"))?;
    let to: Option<Address> = if req.to.is_empty() {
        None
    } else {
        Some(req.to.parse().map_err(|_| Status::invalid_argument("Invalid to address"))?)
    };

    let value = U256::from_str_radix(&req.value, 10).or_else(|_| {
        U256::from_str_radix(req.value.trim_start_matches("0x"), 16)
    }).map_err(|_| Status::invalid_argument("Invalid value"))?;

    Ok(DomainTransactionRequest {
        from,
        to,
        value,
        nonce: req.nonce,
        data: req.data,
        gas_limit: req.gas_limit,
        gas_price: U256::from(req.gas_price),
    })
}

pub fn map_propose_block_request(req: ProposeBlockRequest) -> DomainProposeBlockRequest {
    DomainProposeBlockRequest {
        timestamp: req.timestamp,
    }
}

pub fn map_payload_status(status: DomainPayloadStatus) -> ProtoPayloadStatus {
    ProtoPayloadStatus {
        status: status.status,
        latest_valid_hash: status.latest_valid_hash.map(|h| format!("{:?}", h)).unwrap_or_default(),
        validation_error: status.validation_error.unwrap_or_default(),
    }
}

pub fn map_forkchoice_updated_request(req: ProtoForkchoiceUpdatedRequest) -> Result<DomainForkchoiceUpdatedRequest, Status> {
    let forkchoice_state = req.forkchoice_state.ok_or_else(|| Status::invalid_argument("Missing forkchoice_state"))?;
    
    let payload_attributes = if let Some(attr) = req.payload_attributes {
        Some(DomainPayloadAttributes {
            timestamp: attr.timestamp,
            prev_randao: attr.prev_randao.parse().map_err(|_| Status::invalid_argument("Invalid prev_randao"))?,
            suggested_fee_recipient: attr.suggested_fee_recipient.parse().map_err(|_| Status::invalid_argument("Invalid suggested_fee_recipient"))?,
            withdrawals: Vec::new(), // TODO: map withdrawals if needed
            parent_beacon_block_root: if attr.parent_beacon_block_root.is_empty() {
                None
            } else {
                Some(attr.parent_beacon_block_root.parse().map_err(|_| Status::invalid_argument("Invalid parent_beacon_block_root"))?)
            },
        })
    } else {
        None
    };

    Ok(DomainForkchoiceUpdatedRequest {
        head_block_hash: forkchoice_state.head_block_hash.parse().map_err(|_| Status::invalid_argument("Invalid head_block_hash"))?,
        safe_block_hash: forkchoice_state.safe_block_hash.parse().map_err(|_| Status::invalid_argument("Invalid safe_block_hash"))?,
        finalized_block_hash: forkchoice_state.finalized_block_hash.parse().map_err(|_| Status::invalid_argument("Invalid finalized_block_hash"))?,
        payload_attributes,
    })
}

pub fn map_forkchoice_updated_response(resp: DomainForkchoiceUpdatedResponse) -> ProtoForkchoiceUpdatedResponse {
    ProtoForkchoiceUpdatedResponse {
        payload_status: Some(ProtoPayloadStatus {
            status: resp.status,
            latest_valid_hash: String::new(), // Not provided in DomainForkchoiceUpdatedResponse
            validation_error: String::new(),
        }),
        payload_id: resp.payload_id.map(|id| format!("{:?}", id)).unwrap_or_default(),
    }
}

pub fn map_get_payload_request(req: ProtoGetPayloadRequest) -> Result<DomainGetPayloadRequest, Status> {
    Ok(DomainGetPayloadRequest {
        payload_id: req.payload_id.parse().map_err(|_| Status::invalid_argument("Invalid payload_id"))?,
    })
}

pub fn map_execution_payload_to_proto(payload: DomainExecutionPayload) -> ProtoExecutionPayload {
    ProtoExecutionPayload {
        parent_hash: format!("{:?}", payload.parent_hash),
        fee_recipient: format!("{:?}", payload.fee_recipient),
        state_root: format!("{:?}", payload.state_root),
        receipts_root: format!("{:?}", payload.receipts_root),
        logs_bloom: format!("{:?}", payload.logs_bloom),
        prev_randao: format!("{:?}", payload.prev_randao),
        block_number: payload.block_number,
        gas_limit: payload.gas_limit,
        gas_used: payload.gas_used,
        timestamp: payload.timestamp,
        extra_data: payload.extra_data,
        base_fee_per_gas: payload.base_fee_per_gas.to_string(),
        block_hash: format!("{:?}", payload.block_hash),
        transactions: payload.transactions.iter().map(|t| t.to_vec()).collect(),
        withdrawals: Vec::new(), // TODO: implement if needed
        blob_gas_used: 0,
        excess_blob_gas: 0,
        transactions_root: format!("{:?}", payload.transactions_root),
        withdrawals_root: format!("{:?}", payload.withdrawals_root),
    }
}

pub fn map_proto_execution_payload_to_domain(payload: ProtoExecutionPayload) -> Result<DomainExecutionPayload, Status> {
    // This is more complex because it involves converting back from strings/bytes to strong types
    // TODO For now, minimal implementation or use existing logic if any
    let logs_bloom = if payload.logs_bloom.is_empty() {
        Vec::new()
    } else {
        hex::decode(payload.logs_bloom.trim_start_matches("0x"))
            .map_err(|_| Status::invalid_argument("Invalid logs_bloom hex"))?
    };

    Ok(DomainExecutionPayload {
        parent_hash: payload.parent_hash.parse().map_err(|_| Status::invalid_argument("Invalid parent_hash"))?,
        fee_recipient: payload.fee_recipient.parse().map_err(|_| Status::invalid_argument("Invalid fee_recipient"))?,
        state_root: payload.state_root.parse().map_err(|_| Status::invalid_argument("Invalid state_root"))?,
        receipts_root: payload.receipts_root.parse().map_err(|_| Status::invalid_argument("Invalid receipts_root"))?,
        logs_bloom,
        prev_randao: payload.prev_randao.parse().map_err(|_| Status::invalid_argument("Invalid prev_randao"))?,
        block_number: payload.block_number,
        gas_limit: payload.gas_limit,
        gas_used: payload.gas_used,
        timestamp: payload.timestamp,
        extra_data: payload.extra_data,
        base_fee_per_gas: payload.base_fee_per_gas.parse().unwrap_or_default(),
        block_hash: payload.block_hash.parse().map_err(|_| Status::invalid_argument("Invalid block_hash"))?,
        transactions: Vec::new(), // TODO: implement full conversion
        transactions_root: payload.transactions_root.parse().unwrap_or_default(),
        withdrawals_root: payload.withdrawals_root.parse().unwrap_or_default(),
        withdrawals: Vec::new(),
    })
}

pub fn map_peer_info(peer: DomainPeerInfo) -> ProtoPeerInfo {
    ProtoPeerInfo {
        id: peer.id,
        addr: peer.enode,
        enr: peer.enr,
    }
}

pub fn map_net_add_peer_request(req: ProtoNetAddPeerRequest) -> DomainNetAddPeerRequest {
    DomainNetAddPeerRequest {
        enode: req.addr,
    }
}

pub fn map_node_info_response(resp: DomainNetNodeInfoResponse) -> ProtoNetNodeInfoResponse {
    ProtoNetNodeInfoResponse {
        enr: resp.enr,
        node_id: resp.id,
        listen_addresses: vec![resp.network.listen_addr],
    }
}

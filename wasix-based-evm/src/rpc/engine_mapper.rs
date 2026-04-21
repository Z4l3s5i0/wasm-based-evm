use alloy_consensus::{Block, Header, TxEnvelope as Transaction};
use alloy_primitives::U256;
use alloy_rpc_types::engine::{ExecutionPayloadBodyV1, ExecutionPayloadEnvelopeV2, ExecutionPayloadFieldV2, ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3, ExecutionPayloadV4};

pub struct EngineMapper;

impl EngineMapper {
    pub fn to_execution_payload_v1(block: &Block<Transaction>) -> ExecutionPayloadV1 {
        ExecutionPayloadV1 {
            parent_hash: block.header.parent_hash,
            fee_recipient: block.header.beneficiary,
            state_root: block.header.state_root,
            receipts_root: block.header.receipts_root,
            logs_bloom: block.header.logs_bloom,
            prev_randao: block.header.mix_hash,
            block_number: block.header.number,
            gas_limit: block.header.gas_limit,
            gas_used: block.header.gas_used,
            timestamp: block.header.timestamp,
            extra_data: block.header.extra_data.clone(),
            base_fee_per_gas: U256::from(block.header.base_fee_per_gas.unwrap_or_default()),
            block_hash: block.header.hash_slow(),
            transactions: block.body.transactions.iter().map(|tx| alloy_rlp::encode(tx).into()).collect(),
        }
    }
    pub fn to_execution_payload_v2(block: &Block<Transaction>) -> ExecutionPayloadV2 {
        ExecutionPayloadV2 {
            payload_inner: Self::to_execution_payload_v1(block),
            withdrawals: block.body.withdrawals.clone().map(|w| w.to_vec()).unwrap_or_default(),
        }
    }
    pub fn to_execution_payload_envelope_v2(execution_payload_v2: ExecutionPayloadV2, block_value: U256) -> ExecutionPayloadEnvelopeV2 {
        ExecutionPayloadEnvelopeV2 {
            execution_payload: ExecutionPayloadFieldV2::V2(execution_payload_v2),
            block_value,
        }
    }

    pub fn to_execution_payload_v3(block: &Block<Transaction>) -> ExecutionPayloadV3 {
        ExecutionPayloadV3 {
            payload_inner: ExecutionPayloadV2 {
                payload_inner: Self::to_execution_payload_v1(block),
                withdrawals: block.body.withdrawals.clone().map(|w| w.to_vec()).unwrap_or_default(),
            },
            blob_gas_used: 0,
            excess_blob_gas: 0,
        }
    }

    pub fn to_execution_payload_v4(block: &Block<Transaction>) -> ExecutionPayloadV4 {
        ExecutionPayloadV4 {
            payload_inner: Self::to_execution_payload_v3(block),
            block_access_list: vec![].into(),
        }
    }

    pub fn to_execution_payload_body_v1(block: &Block<Transaction>) -> ExecutionPayloadBodyV1 {
        ExecutionPayloadBodyV1 {
            transactions: block.body.transactions.iter().map(|tx| alloy_rlp::encode(tx).into()).collect(),
            withdrawals: block.body.withdrawals.clone().map(|w| w.to_vec()),
        }
    }

    pub fn payload_v1_to_block(
        payload: &ExecutionPayloadV1,
        transactions: Vec<Transaction>,
        withdrawals: Option<Vec<alloy_rpc_types::Withdrawal>>,
    ) -> Block<Transaction> {
        let header = Header {
            parent_hash: payload.parent_hash,
            beneficiary: payload.fee_recipient,
            state_root: payload.state_root,
            transactions_root: alloy_consensus::proofs::calculate_transaction_root(&transactions),
            receipts_root: payload.receipts_root,
            logs_bloom: payload.logs_bloom,
            mix_hash: payload.prev_randao,
            number: payload.block_number,
            gas_limit: payload.gas_limit,
            gas_used: payload.gas_used,
            timestamp: payload.timestamp,
            extra_data: payload.extra_data.clone(),
            base_fee_per_gas: Some(payload.base_fee_per_gas.to::<u64>()),
            withdrawals_root: withdrawals.as_ref().map(|w| {
                let withdrawals_vec: Vec<_> = w.iter().map(|wi| alloy_eips::eip4895::Withdrawal {
                    index: wi.index,
                    validator_index: wi.validator_index,
                    address: wi.address,
                    amount: wi.amount,
                }).collect();
                alloy_consensus::proofs::calculate_withdrawals_root(&withdrawals_vec)
            }),
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: None,
            ommers_hash: alloy_consensus::EMPTY_OMMER_ROOT_HASH,
            difficulty: U256::ZERO,
            nonce: alloy_primitives::B64::ZERO,
            requests_hash: None,
        };

        Block {
            header,
            body: alloy_consensus::BlockBody {
                transactions,
                ommers: vec![],
                withdrawals: withdrawals.map(|w| {
                    alloy_eips::eip4895::Withdrawals::new(w.into_iter().map(|wi| alloy_eips::eip4895::Withdrawal {
                        index: wi.index,
                        validator_index: wi.validator_index,
                        address: wi.address,
                        amount: wi.amount,
                    }).collect())
                }),
            },
        }
    }
}

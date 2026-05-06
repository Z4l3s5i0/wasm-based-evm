use alloy_consensus::{Block as ConsensusBlock, TxEnvelope as Transaction};
use alloy_rpc_types::{Block, BlockTransactions, Header as RpcHeader};
use alloy_rlp::Encodable;
use alloy_primitives::U256;

pub struct BlockMapper;

impl BlockMapper {
    pub fn to_rpc_block(block: ConsensusBlock<Transaction>, full: bool) -> Block {
        let hash = block.header.hash_slow();
        let header = RpcHeader {
            hash,
            total_difficulty: Some(U256::ZERO),
            size: Some(U256::from(block.length())),
            inner: alloy_consensus::Header {
                parent_hash: block.header.parent_hash,
                ommers_hash: block.header.ommers_hash,
                beneficiary: block.header.beneficiary,
                state_root: block.header.state_root,
                transactions_root: block.header.transactions_root,
                receipts_root: block.header.receipts_root,
                logs_bloom: block.header.logs_bloom,
                difficulty: block.header.difficulty,
                number: block.header.number,
                gas_limit: block.header.gas_limit,
                gas_used: block.header.gas_used,
                timestamp: block.header.timestamp,
                extra_data: block.header.extra_data.clone(),
                mix_hash: block.header.mix_hash,
                nonce: block.header.nonce,
                base_fee_per_gas: block.header.base_fee_per_gas,
                withdrawals_root: block.header.withdrawals_root,
                blob_gas_used: None,
                excess_blob_gas: None,
                parent_beacon_block_root: None,
                requests_hash: None,
            },
        };

        let transactions = if full {
            // Mapping full transactions
            BlockTransactions::Full(block.body.transactions.iter().enumerate().map(|(i, tx)| {
                crate::rpc::transaction_mapper::TransactionMapper::to_rpc_transaction(
                    tx.clone(), 
                    Some((block.header.number, hash, i as u64))
                )
            }).collect())
        } else {
            BlockTransactions::Hashes(block.body.transactions.iter().map(|tx| *tx.hash()).collect())
        };

        Block {
            header,
            transactions,
            uncles: vec![],
            withdrawals: block.body.withdrawals.clone(),
        }
    }
}

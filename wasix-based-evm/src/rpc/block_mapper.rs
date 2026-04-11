use alloy_consensus::{Block as ConsensusBlock, TxEnvelope as Transaction};
use alloy_rpc_types::{Block, BlockTransactions, Header};
use alloy_rlp::Encodable;
use alloy_primitives::U256;

pub struct BlockMapper;

impl BlockMapper {
    pub fn to_rpc_block(block: ConsensusBlock<Transaction>, full: bool) -> Block {
        let hash = block.header.hash_slow();
        let header = Header {
            hash,
            total_difficulty: Some(U256::ZERO),
            size: Some(U256::from(block.length())),
            inner: block.header.clone(),
        };

        let transactions = if full {
            // Mapping full transactions
            BlockTransactions::Full(block.body.transactions.iter().enumerate().map(|(i, tx)| {
                crate::rpc::transaction_mapper::TransactionMapper::to_rpc_transaction(
                    tx.clone(), 
                    Some((block.header.number, hash, i))
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

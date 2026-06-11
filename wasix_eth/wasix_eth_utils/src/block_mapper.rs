use alloy_rlp::Encodable;
use wasix_eth_types::{RpcBlock, RpcHeader, U256, Transaction, BlockTransactions, Block};
use crate::transaction_mapper::TransactionMapper;

pub struct BlockMapper;

impl BlockMapper {
    pub fn to_rpc_block(block: Block<Transaction>, full: bool, chain_config: &wasix_eth_types::ChainConfig, total_difficulty: Option<U256>) -> RpcBlock {
        let fork = wasix_eth_types::Hardfork::get_active_fork_with_total_difficulty(chain_config, block.header.number, block.header.timestamp, total_difficulty);
        let hash = block.header.hash_slow();
        let td = if fork >= wasix_eth_types::Hardfork::London || total_difficulty.is_none() { None } else { total_difficulty };

        let header = RpcHeader {
            hash: Some(hash),
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
            total_difficulty: td,
            size: Some(U256::from(block.length())),
            base_fee_per_gas: if fork >= wasix_eth_types::Hardfork::London { block.header.base_fee_per_gas } else { None },
            withdrawals_root: if fork >= wasix_eth_types::Hardfork::Shanghai { block.header.withdrawals_root } else { None },
            blob_gas_used: if fork >= wasix_eth_types::Hardfork::Cancun { block.header.blob_gas_used } else { None },
            excess_blob_gas: if fork >= wasix_eth_types::Hardfork::Cancun { block.header.excess_blob_gas } else { None },
            parent_beacon_block_root: if fork >= wasix_eth_types::Hardfork::Cancun { block.header.parent_beacon_block_root } else { None },
            requests_hash: if fork >= wasix_eth_types::Hardfork::Prague { block.header.requests_hash } else { None },
        };

        let transactions = if full {
            // Mapping full transactions
            BlockTransactions::Full(block.body.transactions.iter().enumerate().map(|(i, tx)| {
                TransactionMapper::to_rpc_transaction(
                    tx.clone(), 
                    Some((block.header.number, hash, i as u64)),
                    Some(block.header.clone())
                )
            }).collect())
        } else {
            BlockTransactions::Hashes(block.body.transactions.iter().map(|tx| *tx.hash()).collect())
        };

        RpcBlock {
            header,
            transactions,
            uncles: block.body.ommers.iter().map(|h| h.hash_slow()).collect(),
            withdrawals: if fork >= wasix_eth_types::Hardfork::Shanghai { block.body.withdrawals.clone() } else { None },
        }
    }
}

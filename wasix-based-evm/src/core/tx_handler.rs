use crate::misc::error::{RpcError, RpcResult};
use crate::storage::mempool::Mempool;
use crate::storage::traits::StateProvider;
use crate::{debug, error};
use alloy_consensus::{TxEnvelope as Transaction, TxLegacy, transaction::SignerRecoverable};
use alloy_primitives::{Address, B256};
use alloy_rlp::{Encodable, Decodable};
use alloy_rpc_types::TransactionRequest;
use alloy_eips::{BlockId, BlockNumberOrTag};
use crate::rpc::account_manager::AccountManager;
use crate::p2p::peer_manager::PeerManager;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct TxHandler {
    pub state_storage: Arc<dyn StateProvider>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub account_manager: Arc<AccountManager>,
    pub peer_manager: Arc<PeerManager>,
}

impl TxHandler {
    pub fn new(
        state_storage: Arc<dyn StateProvider>,
        mempool: Arc<RwLock<Mempool>>,
        account_manager: Arc<AccountManager>,
        peer_manager: Arc<PeerManager>,
    ) -> Self {
        Self {
            state_storage,
            mempool,
            account_manager,
            peer_manager,
        }
    }

    pub async fn handle_send_transaction(&self, request: TransactionRequest) -> RpcResult<B256> {
        debug!("[TxHandler] handle_send_transaction from: {:?}", request.from);
        
        let from = request.from.ok_or_else(|| RpcError::InvalidParams("from address is required".to_string()))?;
        
        if !self.account_manager.is_managed(&from) {
            return Err(RpcError::AccountNotFound(from));
        }

        let chain_id = self.state_storage.chain_id().await.map_err(|e| RpcError::Internal(e.to_string()))?;
        let nonce = if let Some(n) = request.nonce {
            n
        } else {
            self.state_storage.transaction_count(from, BlockId::Number(BlockNumberOrTag::Latest)).await
                .map_err(|e| RpcError::Internal(e.to_string()))?
        };

        // Note: gas_limit estimation is skipped here as it usually requires calling EthService::estimate_gas
        // which might lead to circular dependencies if not careful. 
        // For now, assume it's provided or handled by the orchestrator.
        let gas_limit = request.gas.unwrap_or(21000);

        let tx = if let (Some(max_fee), Some(max_priority)) = (request.max_fee_per_gas, request.max_priority_fee_per_gas) {
            let tx_1559 = alloy_consensus::TxEip1559 {
                chain_id,
                nonce,
                gas_limit,
                max_fee_per_gas: max_fee,
                max_priority_fee_per_gas: max_priority,
                to: request.to.unwrap_or_default().into(),
                value: request.value.unwrap_or_default(),
                input: request.input.data.clone().unwrap_or_default(),
                access_list: request.access_list.clone().unwrap_or_default(),
            };
            self.account_manager.sign_transaction_1559(&from, tx_1559).await?
        } else {
            let gas_price = request.gas_price.unwrap_or(1_000_000_000u128);
            let tx_legacy = TxLegacy {
                chain_id: Some(chain_id),
                nonce,
                gas_price,
                gas_limit,
                to: request.to.unwrap_or_default().into(),
                value: request.value.unwrap_or_default(),
                input: request.input.data.clone().unwrap_or_default(),
            };
            self.account_manager.sign_transaction(&from, tx_legacy).await?
        };

        self.add_to_mempool_and_broadcast(tx, nonce).await
    }

    pub async fn handle_send_raw_transaction(&self, data: String) -> RpcResult<B256> {
        let bytes = hex::decode(data.trim_start_matches("0x"))
            .map_err(|e| RpcError::InvalidParams(format!("Invalid hex: {}", e)))?;
            
        let tx = Transaction::decode(&mut &bytes[..])
            .map_err(|e| RpcError::InvalidParams(format!("Failed to decode transaction: {}", e)))?;
            
        let from = tx.recover_signer()
            .map_err(|e| RpcError::InvalidParams(format!("Failed to recover signer: {}", e)))?;
            
        let nonce = if let Some(n) = self.get_transaction_nonce(&tx) {
            n
        } else {
            self.state_storage.transaction_count(from, BlockId::Number(BlockNumberOrTag::Latest)).await
                .map_err(|e| RpcError::Internal(e.to_string()))?
        };

        self.add_to_mempool_and_broadcast(tx, nonce).await
    }

    pub async fn add_to_mempool_and_broadcast(&self, tx: Transaction, nonce: u64) -> RpcResult<B256> {
        let hash = tx.hash().clone();
        debug!("[TxHandler] Adding transaction {:?} to mempool", hash);
        self.mempool.write().await.add_transaction(tx.clone(), nonce);

        let mut rlp_data = Vec::new();
        tx.encode(&mut rlp_data);
        if !rlp_data.is_empty() {
            debug!("[TxHandler] Gossiping transaction {:?}", hash);
            self.peer_manager.broadcast_gossip(rlp_data).await;
        } else {
            error!("[TxHandler] Failed to RLP-encode transaction {:?}", hash);
        }

        Ok(hash)
    }

    fn get_transaction_nonce(&self, tx: &Transaction) -> Option<u64> {
        match tx {
            Transaction::Legacy(t) => Some(t.tx().nonce),
            Transaction::Eip2930(t) => Some(t.tx().nonce),
            Transaction::Eip1559(t) => Some(t.tx().nonce),
            _ => None,
        }
    }
}

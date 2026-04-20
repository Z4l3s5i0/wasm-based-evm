use std::sync::Arc;
use alloy_primitives::{U256, Address, Bytes};
use alloy_rpc_types::{SyncStatus, TransactionRequest};
use crate::storage::traits::{BlockProvider, StateProvider};
use alloy_eips::{BlockId, BlockNumberOrTag};
use tokio::sync::RwLock;
use crate::mempool::Mempool;
use crate::rpc::account_manager::AccountManager;
use crate::p2p::peer_manager::PeerManager;
use alloy_consensus::{TxEnvelope as Transaction, TxLegacy, transaction::SignerRecoverable, TxReceipt};
use alloy_rlp::{Encodable, Decodable};
use crate::storage::storage::InMemoryStorage;
use evm::standard::TransactValueCallCreate;

use crate::{info, error};
use crate::evm::executor::Executor;
use crate::misc::error;
use crate::misc::error::RpcResult;
use crate::sync::controller::SyncController;

#[derive(Clone)]
pub struct EthService {
    pub block_storage: Arc<dyn BlockProvider>,
    pub state_storage: Arc<dyn StateProvider>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub peer_manager: Arc<PeerManager>,
    pub executor: Executor,
    pub storage: Arc<RwLock<InMemoryStorage>>,
    pub account_manager: Arc<AccountManager>,
    pub sync_engine: Arc<SyncController>,
}

impl EthService {
    pub async fn gas_price(&self) -> RpcResult<U256> {
        // A real client might return the 60th percentile of gas prices from recent blocks
        // or base fee + a standard priority fee.
        if let Ok(Some(header)) = self.block_storage.header(BlockId::Number(BlockNumberOrTag::Latest)).await {
            if let Some(base_fee) = header.base_fee_per_gas {
                // Return base_fee + 1.5 Gwei priority fee as a reasonable default
                let priority_fee = 1_500_000_000u64;
                return Ok(U256::from(base_fee + priority_fee));
            }
        }
        // Fallback to 1 Gwei if no block data found
        Ok(U256::from(1_000_000_000u64))
    }

    pub async fn accounts(&self) -> RpcResult<Vec<Address>> {
        self.state_storage.accounts().await.map_err(|e| error::RpcError::Internal(e.to_string()))
    }

    pub async fn syncing(&self) -> RpcResult<SyncStatus> {
        Ok(self.sync_engine.status().await)
    }

    pub async fn send_transaction(&self, request: TransactionRequest) -> RpcResult<alloy_primitives::B256> {
        info!("[EthService] eth_sendTransaction from: {:?}", request.from);
        
        let from = request.from.ok_or_else(|| error::RpcError::InvalidParams("from address is required".to_string()))?;
        
        if !self.account_manager.is_managed(&from) {
            return Err(error::RpcError::AccountNotFound(from));
        }

        let chain_id = self.block_storage.chain_id().await.map_err(|e| error::RpcError::Internal(e.to_string()))?;
        let nonce = if let Some(n) = request.nonce {
            n
        } else {
            self.state_storage.transaction_count(from, BlockId::Number(BlockNumberOrTag::Latest)).await
                .map_err(|e| error::RpcError::Internal(e.to_string()))?
        };

        // For eth_sendTransaction, we should handle gas and gas_price properly
        let gas_limit = if let Some(g) = request.gas {
            g
        } else {
            // For simple transfers use 21000, else estimate
            if request.to.is_some() && request.input.data.is_none() {
                21000
            } else {
                self.estimate_gas(request.clone(), None).await?.to::<u64>()
            }
        };

        let tx = if let (Some(max_fee), Some(max_priority)) = (request.max_fee_per_gas, request.max_priority_fee_per_gas) {
            // EIP-1559
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
            let signed_tx: Transaction = self.account_manager.sign_transaction_1559(&from, tx_1559).await?;
            signed_tx
        } else {
            // Legacy
            let gas_price = if let Some(p) = request.gas_price {
                p
            } else {
                self.gas_price().await?.to::<u128>()
            };
            let tx_legacy = TxLegacy {
                chain_id: Some(chain_id),
                nonce,
                gas_price,
                gas_limit,
                to: request.to.unwrap_or_default().into(),
                value: request.value.unwrap_or_default(),
                input: request.input.data.clone().unwrap_or_default(),
            };
            let signed_tx: Transaction = self.account_manager.sign_transaction(&from, tx_legacy).await?;
            signed_tx
        };

        let hash = tx.hash().clone();
        info!("[EthService] Adding signed transaction {:?} to mempool", hash);
        self.mempool.write().await.add_transaction(tx.clone(), nonce);

        // Gossip the transaction to the P2P network
        let mut rlp_data = Vec::new();
        tx.encode(&mut rlp_data);
        if !rlp_data.is_empty() {
            info!("[EthService] Gossiping transaction {:?}", hash);
            self.peer_manager.broadcast_gossip(rlp_data).await;
        } else {
            error!("[EthService] Failed to RLP-encode transaction {:?}", hash);
        }

        Ok(hash)
    }

    pub async fn call(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<Bytes> {
        info!("[EthService] eth_call: to={:?}, data_len={}", request.to, request.input.data.as_ref().map(|d| d.len()).unwrap_or(0));
        
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let block = self.block_storage.block(block_id).await
            .map_err(|e| error::RpcError::Internal(e.to_string()))?
            .ok_or(error::RpcError::BlockNotFound(block_id))?;
        
        // Use a test signature for simulation since it won't be verified for `call`
        let signature = alloy_primitives::Signature::test_signature();
        let hash = alloy_primitives::B256::ZERO;
        
        let tx_envelope = if let (Some(max_fee), Some(max_priority)) = (request.max_fee_per_gas, request.max_priority_fee_per_gas) {
            let tx = alloy_consensus::TxEip1559 {
                chain_id: self.block_storage.chain_id().await.map_err(|e| error::RpcError::Internal(e.to_string()))?,
                nonce: request.nonce.unwrap_or_default(),
                gas_limit: request.gas.unwrap_or(30_000_000), // High default for call
                max_fee_per_gas: max_fee,
                max_priority_fee_per_gas: max_priority,
                to: request.to.unwrap_or_default().into(),
                value: request.value.unwrap_or_default(),
                input: request.input.data.clone().unwrap_or_default(),
                access_list: request.access_list.clone().unwrap_or_default(),
            };
            Transaction::Eip1559(alloy_consensus::Signed::new_unchecked(tx, signature, hash))
        } else {
            let tx = TxLegacy {
                chain_id: Some(self.block_storage.chain_id().await.map_err(|e| error::RpcError::Internal(e.to_string()))?),
                nonce: request.nonce.unwrap_or_default(),
                gas_price: request.gas_price.unwrap_or(1_000_000_000u128),
                gas_limit: request.gas.unwrap_or(30_000_000), // High default for call
                to: request.to.unwrap_or_default().into(),
                value: request.value.unwrap_or_default(),
                input: request.input.data.clone().unwrap_or_default(),
            };
            Transaction::Legacy(alloy_consensus::Signed::new_unchecked(tx, signature, hash))
        };

        let storage_lock = self.storage.read().await;

        let result = self.executor.run_execution(&mut storage_lock.clone(), vec![tx_envelope], block, false)
            .map(|mut v| v.remove(0)).map_err(|e| error::RpcError::Internal(e))?;
        
        match result.call_create {
            TransactValueCallCreate::Call { retval, .. } => Ok(retval.into()),
            TransactValueCallCreate::Create { .. } => Ok(Bytes::new()),
        }
    }

    pub async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<U256> {
        info!("[EthService] eth_estimateGas: to={:?}, data_len={}", request.to, request.input.data.as_ref().map(|d| d.len()).unwrap_or(0));
        
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let block = self.block_storage.block(block_id).await
            .map_err(|e| error::RpcError::Internal(e.to_string()))?
            .ok_or(error::RpcError::BlockNotFound(block_id))?;
        
        let signature = alloy_primitives::Signature::test_signature();
        let hash = alloy_primitives::B256::ZERO;
        
        let tx_envelope = if let (Some(max_fee), Some(max_priority)) = (request.max_fee_per_gas, request.max_priority_fee_per_gas) {
            let tx = alloy_consensus::TxEip1559 {
                chain_id: self.block_storage.chain_id().await.map_err(|e| error::RpcError::Internal(e.to_string()))?,
                nonce: request.nonce.unwrap_or_default(),
                gas_limit: request.gas.unwrap_or(30_000_000), // Use block gas limit or a high enough value
                max_fee_per_gas: max_fee,
                max_priority_fee_per_gas: max_priority,
                to: request.to.unwrap_or_default().into(),
                value: request.value.unwrap_or_default(),
                input: request.input.data.clone().unwrap_or_default(),
                access_list: request.access_list.clone().unwrap_or_default(),
            };
            Transaction::Eip1559(alloy_consensus::Signed::new_unchecked(tx, signature, hash))
        } else {
            let tx = TxLegacy {
                chain_id: Some(self.block_storage.chain_id().await.map_err(|e| error::RpcError::Internal(e.to_string()))?),
                nonce: request.nonce.unwrap_or_default(),
                gas_price: request.gas_price.unwrap_or(1_000_000_000u128),
                gas_limit: request.gas.unwrap_or(30_000_000), 
                to: request.to.unwrap_or_default().into(),
                value: request.value.unwrap_or_default(),
                input: request.input.data.clone().unwrap_or_default(),
            };
            Transaction::Legacy(alloy_consensus::Signed::new_unchecked(tx, signature, hash))
        };

        let storage_lock = self.storage.read().await;

        let result = self.executor.run_execution(&mut storage_lock.clone(), vec![tx_envelope], block, false).map(|mut v| v.remove(0))
            .map_err(|e| error::RpcError::Internal(e))?;

        Ok(U256::from(result.used_gas.as_u64()))
    }

    pub async fn send_raw_transaction(&self, data: String) -> RpcResult<alloy_primitives::B256> {
        let data = hex::decode(data.trim_start_matches("0x"))
            .map_err(|e| error::RpcError::InvalidParams(format!("Invalid hex: {}", e)))?;
        
        let signed_tx = Transaction::decode(&mut &data[..])
            .map_err(|e| error::RpcError::InvalidParams(format!("Invalid RLP: {}", e)))?;
        
        let hash = signed_tx.hash().clone();
        info!("[EthService] eth_sendRawTransaction hash: {:?}", hash);

        let from = signed_tx.recover_signer().unwrap_or_default();
        let current_nonce = self.state_storage.transaction_count(from, BlockId::Number(BlockNumberOrTag::Latest)).await
            .map_err(|e| error::RpcError::Internal(e.to_string()))?;

        self.mempool.write().await.add_transaction(signed_tx.clone(), current_nonce);

        // Gossip the transaction to the P2P network
        let mut rlp_data = Vec::new();
        signed_tx.encode(&mut rlp_data);
        if !rlp_data.is_empty() {
            info!("[EthService] Gossiping transaction {:?}", hash);
            self.peer_manager.broadcast_gossip(rlp_data).await;
        }

        Ok(hash)
    }

    pub async fn sign_transaction(&self, request: TransactionRequest) -> RpcResult<Bytes> {
        let from = request.from.ok_or_else(|| error::RpcError::InvalidParams("from address is required".to_string()))?;
        
        if !self.account_manager.is_managed(&from) {
            return Err(error::RpcError::AccountNotFound(from));
        }

        let chain_id = self.block_storage.chain_id().await.map_err(|e| error::RpcError::Internal(e.to_string()))?;
        let nonce = if let Some(n) = request.nonce {
            n
        } else {
            self.state_storage.transaction_count(from, BlockId::Number(BlockNumberOrTag::Latest)).await
                .map_err(|e| error::RpcError::Internal(e.to_string()))?
        };

        let gas_limit = request.gas.unwrap_or(21000);

        let signed_tx = if let (Some(max_fee), Some(max_priority)) = (request.max_fee_per_gas, request.max_priority_fee_per_gas) {
            let tx = alloy_consensus::TxEip1559 {
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
            self.account_manager.sign_transaction_1559(&from, tx).await?
        } else {
            let gas_price = request.gas_price.unwrap_or(1_000_000_000u128);
            let tx = TxLegacy {
                chain_id: Some(chain_id),
                nonce,
                gas_price,
                gas_limit,
                to: request.to.unwrap_or_default().into(),
                value: request.value.unwrap_or_default(),
                input: request.input.data.clone().unwrap_or_default(),
            };
            self.account_manager.sign_transaction(&from, tx).await?
        };

        let mut rlp_data = Vec::new();
        signed_tx.encode(&mut rlp_data);
        
        Ok(Bytes::from(rlp_data))
    }

    pub async fn sign(&self, address: Address, message: String) -> RpcResult<alloy_primitives::Signature> {
        if !self.account_manager.is_managed(&address) {
            return Err(error::RpcError::AccountNotFound(address));
        }

        // eth_sign expects the message to be signed with the Ethereum prefix
        // However, many implementations just sign the raw message if it's already hex, 
        // or the bytes of the string.
        // The spec says: "The message is prefixed with "\x19Ethereum Signed Message:\n" + message.length and hashed"
        // PrivateKeySigner::sign_message_sync already handles EIP-191 prefixing.
        
        let message_bytes = if message.starts_with("0x") {
            hex::decode(&message[2..]).unwrap_or_else(|_| message.into_bytes())
        } else {
            message.into_bytes()
        };

        self.account_manager.sign(&address, &message_bytes).await
    }
}

use std::sync::Arc;
use crate::error::RpcResult;
use alloy_primitives::{U256, Address, Bytes};
use alloy_rpc_types::{SyncStatus, TransactionRequest};
use crate::storage::traits::{BlockProvider, StateProvider};
use alloy_eips::{BlockId, BlockNumberOrTag};
use tokio::sync::RwLock;
use crate::mempool::Mempool;
use crate::rpc::account_manager::AccountManager;
use crate::p2p::peer_manager::PeerManager;
use crate::executor::Executor;
use alloy_consensus::{TxEnvelope as Transaction, TxLegacy, transaction::SignerRecoverable};
use alloy_rlp::{Encodable, Decodable};
use crate::storage::storage::InMemoryStorage;
use evm::standard::TransactValueCallCreate;

use crate::{info, error};

use crate::p2p::sync::SyncEngine;

pub struct EthService {
    pub block_storage: Arc<dyn BlockProvider>,
    pub state_storage: Arc<dyn StateProvider>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub peer_manager: Arc<PeerManager>,
    pub executor: Executor,
    pub storage: Arc<RwLock<InMemoryStorage>>,
    pub account_manager: Arc<AccountManager>,
    pub sync_engine: Arc<SyncEngine>,
}

impl EthService {
    pub async fn gas_price(&self) -> RpcResult<U256> {
        //TODO: Implement gas price
        if let Ok(Some(header)) = self.block_storage.header(BlockId::Number(BlockNumberOrTag::Latest)).await {
            if let Some(base_fee) = header.base_fee_per_gas {
                return Ok(U256::from(base_fee));
            }
        }
        // Fallback to 1 Gwei
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

        // Standard gas price if not provided: use the current gas price
        let gas_price = if let Some(p) = request.gas_price {
            p
        } else {
            self.gas_price().await?.to::<u128>()
        };

        // Standard gas limit if not provided: estimate gas or use 21000 for simple transfers
        let gas_limit = if let Some(g) = request.gas {
            g
        } else {
            // For now, simple 21000 if it's just a transfer, otherwise we should estimate
            if request.to.is_some() && request.input.data.is_none() {
                21000
            } else {
                self.estimate_gas(request.clone(), None).await?.to::<u64>()
            }
        };

        let tx = TxLegacy {
            chain_id: Some(chain_id),
            nonce,
            gas_price,
            gas_limit,
            to: request.to.unwrap_or_default().into(),
            value: request.value.unwrap_or_default(),
            input: request.input.data.clone().unwrap_or_default(),
        };

        let signed_tx: Transaction = self.account_manager.sign_transaction(&from, tx).await?;
        let hash = signed_tx.hash().clone();

        info!("[EthService] Adding signed transaction {:?} to mempool", hash);
        self.mempool.write().await.add_transaction(signed_tx.clone(), nonce);

        // Gossip the transaction to the P2P network
        let mut rlp_data = Vec::new();
        signed_tx.encode(&mut rlp_data);
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
        // TODO: Implement eth_call
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let block = self.block_storage.block(block_id).await
            .map_err(|e| error::RpcError::Internal(e.to_string()))?
            .ok_or(error::RpcError::BlockNotFound(block_id))?;
        
        // Convert TransactionRequest to TxEnvelope
        // This is simplified, we need a way to create a TxEnvelope from a request for simulation
        let tx = TxLegacy {
            chain_id: Some(self.block_storage.chain_id().await.map_err(|e| error::RpcError::Internal(e.to_string()))?),
            nonce: request.nonce.unwrap_or_default(),
            gas_price: request.gas_price.unwrap_or(1_000_000_000u128),
            gas_limit: request.gas.unwrap_or(1_000_000),
            to: request.to.unwrap_or_default().into(),
            value: request.value.unwrap_or_default(),
            input: request.input.data.clone().unwrap_or_default(),
        };
        let tx_envelope = Transaction::Legacy(alloy_consensus::Signed::new_unchecked(tx, alloy_primitives::Signature::test_signature(), alloy_primitives::B256::ZERO));

        let storage_lock = self.storage.read().await;
        // In a real implementation, we should execute on top of the state of the given block.
        // InMemoryStorage currently only has the latest state easily accessible for execution.
        let result = self.executor.call(&storage_lock, tx_envelope, block)
            .map_err(|e| error::RpcError::Internal(e))?;
        
        match result.call_create {
            TransactValueCallCreate::Call { retval, .. } => Ok(retval.into()),
            TransactValueCallCreate::Create { .. } => Ok(Bytes::new()), // For create, returns init code or empty? Usually call on address
        }
    }

    pub async fn estimate_gas(&self, request: TransactionRequest, block_id: Option<BlockId>) -> RpcResult<U256> {
        // TODO: Implement gas estimation
        // Similar to call, but we return gas used.
        // This is a simplified version.
        let block_id = block_id.unwrap_or(BlockId::Number(BlockNumberOrTag::Latest));
        let _block = self.block_storage.block(block_id).await
            .map_err(|e| crate::error::RpcError::Internal(e.to_string()))?
            .ok_or(crate::error::RpcError::BlockNotFound(block_id))?;
        
        let tx = TxLegacy {
            chain_id: Some(self.block_storage.chain_id().await.map_err(|e| crate::error::RpcError::Internal(e.to_string()))?),
            nonce: request.nonce.unwrap_or_default(),
            gas_price: request.gas_price.unwrap_or(1_000_000_000u128),
            gas_limit: request.gas.unwrap_or(10_000_000), // High limit for estimation
            to: request.to.unwrap_or_default().into(),
            value: request.value.unwrap_or_default(),
            input: request.input.data.clone().unwrap_or_default(),
        };
        let _tx_envelope = Transaction::Legacy(alloy_consensus::Signed::new_unchecked(tx, alloy_primitives::Signature::test_signature(), alloy_primitives::B256::ZERO));

        let _storage_lock = self.storage.read().await;
        // For estimation, we need to know gas used.
        // Our executor currently returns TransactValue which is just the output bytes.
        // We might need to modify Executor to return more info.
        // For now, let's return a dummy gas value or modify the executor.
        
        // Let's assume 21000 + some gas for now, or use a fixed value.
        Ok(U256::from(21000u64))
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

        let gas_price = request.gas_price.unwrap_or(1_000_000_000u128);
        let gas_limit = request.gas.unwrap_or(21000);

        let tx = TxLegacy {
            chain_id: Some(chain_id),
            nonce,
            gas_price,
            gas_limit,
            to: request.to.unwrap_or_default().into(),
            value: request.value.unwrap_or_default(),
            input: request.input.data.clone().unwrap_or_default(),
        };

        let signed_tx = self.account_manager.sign_transaction(&from, tx).await?;
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

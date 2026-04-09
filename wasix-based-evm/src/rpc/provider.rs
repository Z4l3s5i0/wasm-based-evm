use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::{Mutex, oneshot, mpsc};
use alloy_primitives::{Address, B256};
use alloy_rlp::Decodable;

use crate::executor::Executor;
use crate::storage::storage::InMemoryStorage;
use crate::storage::types::{Block, Transaction, PendingPayload, Receipt};
use crate::rpc::provider_error::ProviderError;
use crate::rpc::provider_api::*;
use crate::network::NetworkMessage;
use crate::rpc::evm_rpc::{TransactionRequest, ProposeBlockRequest, ForkchoiceUpdatedRequest, GetPayloadRequest, PeerInfo as ProtoPeerInfo, ExecutionPayload as ProtoExecutionPayload};

pub struct DefaultBlockchainProvider {
    pub storage: Arc<Mutex<InMemoryStorage>>,
    pub executor: Executor,
    pub network_send: Option<mpsc::Sender<NetworkMessage>>,
    pub tx_broadcast: Option<mpsc::Sender<Transaction>>,
    pub pending_payloads: Arc<Mutex<std::collections::HashMap<String, PendingPayload>>>,
}

impl DefaultBlockchainProvider {
    pub fn new(
        storage: Arc<Mutex<InMemoryStorage>>,
        executor: Executor,
        network_send: Option<mpsc::Sender<NetworkMessage>>,
        tx_broadcast: Option<mpsc::Sender<Transaction>>,
        pending_payloads: Arc<Mutex<std::collections::HashMap<String, PendingPayload>>>,
    ) -> Self {
        Self {
            storage,
            executor,
            network_send,
            tx_broadcast,
            pending_payloads,
        }
    }
}

#[async_trait]
impl EthReadProvider for DefaultBlockchainProvider {
    async fn accounts(&self) -> Result<Vec<Address>, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_accounts())
    }

    async fn latest_block_number(&self) -> Result<u64, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_latest_block_number())
    }

    async fn balance(&self, address: Address) -> Result<u128, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_balance(address).to::<u128>())
    }

    async fn block_by_number(&self, number: u64) -> Result<Option<Block>, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_block_by_number(number).cloned())
    }

    async fn block_by_hash(&self, hash: B256) -> Result<Option<Block>, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_block_by_hash(hash).cloned())
    }

    async fn block_transaction_count_by_number(&self, number: u64) -> Result<u64, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_block_by_number(number)
            .map(|b| b.body.execution_payload.transactions.len() as u64)
            .unwrap_or(0))
    }

    async fn block_transaction_count_by_hash(&self, hash: B256) -> Result<u64, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_block_by_hash(hash)
            .map(|b| b.body.execution_payload.transactions.len() as u64)
            .unwrap_or(0))
    }

    async fn tx_by_hash(&self, hash: B256) -> Result<Option<Transaction>, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_transaction_by_hash(hash).cloned())
    }

    async fn tx_receipt_by_hash(&self, hash: B256) -> Result<Option<(Transaction, Receipt, Block)>, ProviderError> {
        let storage = self.storage.lock().await;
        let receipt = match storage.get_receipt_by_tx_hash(hash) {
            Some(r) => r.clone(),
            None => return Ok(None),
        };
        let tx = match storage.get_transaction_by_hash(hash) {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        let block = match storage.get_block_by_number(receipt.block_number) {
            Some(b) => b.clone(),
            None => return Ok(None),
        };
        Ok(Some((tx, receipt, block)))
    }

    async fn code_at(&self, address: Address) -> Result<Vec<u8>, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_code(address))
    }

    async fn roots(&self) -> Result<(B256, B256, Option<B256>), ProviderError> {
        let storage = self.storage.lock().await;
        let latest_num = storage.get_latest_block_number();
        let latest_block = storage.get_block_by_number(latest_num);
        let receipts_root = latest_block.map(|b| b.body.execution_payload.receipts_root).unwrap_or(B256::ZERO);
        Ok((storage.calculate_state_root(), receipts_root, latest_block.map(|b| b.body.execution_payload.block_hash)))
    }

    async fn mempool(&self) -> Result<Vec<Transaction>, ProviderError> {
        let storage = self.storage.lock().await;
        Ok(storage.get_mempool().cloned().collect())
    }
}

#[async_trait]
impl EthWriteProvider for DefaultBlockchainProvider {
    async fn send_transaction(&self, req: TransactionRequest) -> Result<Transaction, ProviderError> {
        let tx = Transaction::try_from(req)?;

        // Broadcast to network if handle is available
        if let Some(ref tx_broadcast) = self.tx_broadcast {
            let _ = tx_broadcast.send(tx.clone()).await;
        }

        let mut storage = self.storage.lock().await;
        storage.add_transaction(tx.clone());
        Ok(tx)
    }

    async fn call(&self, req: TransactionRequest) -> Result<Transaction, ProviderError> {
        let tx = Transaction::try_from(req)?;
        Ok(tx)
    }
}

#[async_trait]
impl EngineProvider for DefaultBlockchainProvider {
    async fn propose_block(&self, req: ProposeBlockRequest) -> Result<ProposeBlockResult, ProviderError> {
        let address: Address = req.fee_recipient.parse().map_err(|_| ProviderError::InvalidInput("Invalid address".to_string()))?;

        let (mut storage, executor) = {
            let storage = self.storage.lock().await;
            (storage, self.executor.clone())
        };

        // For propose_block, we build a new block from mempool or provided txs
        let mut transactions = Vec::new();
        if req.from_mempool {
            let n = if req.max_transactions == 0 { 100 } else { req.max_transactions as usize };
            transactions = storage.mempool.pop_transactions(n);
        }

        for tx_req in req.transactions {
            let tx = Transaction::try_from(tx_req)?;
            transactions.push(tx);
        }

        let latest_block = storage.get_latest_block().cloned().expect("Genesis block should exist");
        
        let parent_hash = if req.parent_hash.is_empty() {
            latest_block.body.execution_payload.block_hash
        } else {
            req.parent_hash.parse().map_err(|_| ProviderError::InvalidInput("Invalid parent hash".to_string()))?
        };

        let timestamp = if req.timestamp == 0 {
            latest_block.body.execution_payload.timestamp + 12
        } else {
            req.timestamp
        };

        let block_builder = Block::builder(req.slot)
            .parent_hash(parent_hash)
            .timestamp(timestamp)
            .fee_recipient(address);

        let _ = executor.execute_block(&mut storage, transactions.clone(), block_builder.build())
            .map_err(|e| ProviderError::Execution(e))?;

        // Get the actual block that was added to storage (with calculated roots)
        let block = storage.get_latest_block().cloned().ok_or_else(|| ProviderError::Internal("Block not found after execution".to_string()))?;
        let block_hash = block.body.execution_payload.block_hash;

        // Broadcast block
        if let Some(ref network_send) = self.network_send {
            let _ = network_send.send(NetworkMessage::BroadcastBlock(block)).await;
        }

        Ok(ProposeBlockResult {
            block_hash,
            tx_results: transactions,
        })
    }

    async fn engine_new_payload(&self, payload: ProtoExecutionPayload) -> Result<crate::rpc::evm_rpc::PayloadStatus, ProviderError> {
        let mut storage = self.storage.lock().await;
        let executor = self.executor.clone();

        let block_hash: B256 = payload.block_hash.parse().map_err(|_| ProviderError::InvalidInput("Invalid block hash".to_string()))?;
        if storage.get_block_by_hash(block_hash).is_some() {
            return Ok(crate::rpc::evm_rpc::PayloadStatus {
                status: "VALID".to_string(),
                latest_valid_hash: format!("{:?}", block_hash),
                validation_error: String::new(),
            });
        }

        // Decode transactions
        let mut transactions = Vec::new();
        for data in &payload.transactions {
            match Transaction::decode(&mut data.as_ref()) {
                Ok(tx) => transactions.push(tx),
                Err(e) => {
                    return Ok(crate::rpc::evm_rpc::PayloadStatus {
                        status: "INVALID".to_string(),
                        latest_valid_hash: String::new(),
                        validation_error: format!("Decode error: {:?}", e),
                    });
                }
            }
        }

        let parent_hash: B256 = payload.parent_hash.parse().map_err(|_| ProviderError::InvalidInput("Invalid parent hash".to_string()))?;

        let block = Block::builder(payload.block_number)
            .parent_hash(parent_hash)
            .timestamp(payload.timestamp)
            .fee_recipient(payload.fee_recipient.parse().unwrap_or_default())
            .gas_limit(payload.gas_limit)
            .gas_used(payload.gas_used)
            .block_hash(block_hash)
            .transactions(transactions.clone())
            .build();

        // Execute block
        match executor.execute_block(&mut storage, transactions, block.clone()) {
            Ok(_) => {
                storage.add_block(block.clone());
                // Broadcast block
                if let Some(ref network_send) = self.network_send {
                    let _ = network_send.send(NetworkMessage::BroadcastBlock(block)).await;
                }

                Ok(crate::rpc::evm_rpc::PayloadStatus {
                    status: "VALID".to_string(),
                    latest_valid_hash: format!("{:?}", block_hash),
                    validation_error: String::new(),
                })
            }
            Err(e) => {
                Ok(crate::rpc::evm_rpc::PayloadStatus {
                    status: "INVALID".to_string(),
                    latest_valid_hash: String::new(),
                    validation_error: e,
                })
            }
        }
    }

    async fn engine_forkchoice_updated(&self, req: ForkchoiceUpdatedRequest) -> Result<crate::rpc::evm_rpc::ForkchoiceUpdatedResponse, ProviderError> {
        let head_block_hash: B256 = req.forkchoice_state.as_ref()
            .map(|s| s.head_block_hash.parse().unwrap_or_default())
            .unwrap_or_default();

        // 1. Update forkchoice in storage
        {
            let mut storage = self.storage.lock().await;
            storage.update_forkchoice(head_block_hash);
        }

        // 2. If payload_attributes is present, start building a new block
        let payload_id = if let Some(attr) = req.payload_attributes {
            let mut storage = self.storage.lock().await;
            let executor = self.executor.clone();

            let fee_recipient: Address = attr.suggested_fee_recipient.parse().map_err(|_| ProviderError::InvalidInput("Invalid fee recipient".to_string()))?;

            let latest_block = storage.get_block_by_hash(head_block_hash).cloned().unwrap_or_else(|| storage.get_latest_block().cloned().unwrap());
            let next_number = latest_block.body.execution_payload.block_number + 1;
            let txs = storage.mempool.pop_transactions(10);
            
            let block_to_execute = Block::builder(next_number)
                .parent_hash(head_block_hash)
                .timestamp(attr.timestamp)
                .fee_recipient(fee_recipient)
                .transactions(txs.clone())
                .build();

            // Build block (dry run execution to get payload)
            let (_results, receipts, changeset) = executor.execute_with_changeset(&mut storage, txs, block_to_execute.clone())
                .map_err(|e| ProviderError::Execution(e))?;

            let id = format!("{:x}", head_block_hash); // Simplified ID

            let mut pending = self.pending_payloads.lock().await;
            pending.insert(id.clone(), PendingPayload {
                block: block_to_execute,
                receipts,
                total_changeset: changeset,
            });

            Some(id)
        } else {
            None
        };

        Ok(crate::rpc::evm_rpc::ForkchoiceUpdatedResponse {
            payload_status: Some(crate::rpc::evm_rpc::PayloadStatus {
                status: "VALID".to_string(),
                latest_valid_hash: req.forkchoice_state.as_ref().map(|s| s.head_block_hash.clone()).unwrap_or_default(),
                validation_error: String::new(),
            }),
            payload_id: payload_id.unwrap_or_default(),
        })
    }

    async fn engine_get_payload(&self, req: GetPayloadRequest) -> Result<ProtoExecutionPayload, ProviderError> {
        let mut pending = self.pending_payloads.lock().await;
        let payload = pending.remove(&req.payload_id).ok_or_else(|| ProviderError::NotFound("Payload not found"))?;
        let p = payload.block.body.execution_payload;
        
        Ok(ProtoExecutionPayload {
            parent_hash: format!("{:?}", p.parent_hash),
            fee_recipient: format!("{:?}", p.fee_recipient),
            state_root: format!("{:?}", p.state_root),
            receipts_root: format!("{:?}", p.receipts_root),
            logs_bloom: format!("{:?}", p.logs_bloom),
            prev_randao: format!("{:?}", p.prev_randao),
            block_number: p.block_number,
            gas_limit: p.gas_limit,
            gas_used: p.gas_used,
            timestamp: p.timestamp,
            extra_data: p.extra_data,
            base_fee_per_gas: p.base_fee_per_gas.to_string(),
            block_hash: format!("{:?}", p.block_hash),
            transactions: p.transactions.iter().map(|t| t.to_vec()).collect(),
            withdrawals: Vec::new(), // TODO: map withdrawals if needed
            blob_gas_used: 0,
            excess_blob_gas: 0,
            transactions_root: format!("{:?}", p.transactions_root),
            withdrawals_root: String::new(),
        })
    }
}

#[async_trait]
impl NetProvider for DefaultBlockchainProvider {
    async fn peer_count(&self) -> Result<u64, ProviderError> {
        let (tx, rx) = oneshot::channel();
        if let Some(ref network_send) = self.network_send {
            network_send.send(NetworkMessage::GetPeerCount(tx)).await
                .map_err(|_| ProviderError::Internal("Network sender dropped".to_string()))?;
            Ok(rx.await.map_err(|_| ProviderError::Internal("Oneshot dropped".to_string()))? as u64)
        } else {
            Err(ProviderError::Internal("Network handle not available".to_string()))
        }
    }

    async fn peers(&self) -> Result<Vec<crate::rpc::evm_rpc::PeerInfo>, ProviderError> {
        let (tx, rx) = oneshot::channel();
        if let Some(ref network_send) = self.network_send {
            network_send.send(NetworkMessage::GetPeers(tx)).await
                .map_err(|_| ProviderError::Internal("Network sender dropped".to_string()))?;
            let peers = rx.await.map_err(|_| ProviderError::Internal("Oneshot dropped".to_string()))?;
            Ok(peers.into_iter().map(|p| ProtoPeerInfo {
                id: p.id,
                addr: p.addr,
                enr: p.enr.unwrap_or_default(),
            }).collect())
        } else {
            Err(ProviderError::Internal("Network handle not available".to_string()))
        }
    }

    async fn add_peer(&self, req: crate::rpc::evm_rpc::NetAddPeerRequest) -> Result<(), ProviderError> {
        let (tx, rx) = oneshot::channel();
        if let Some(ref network_send) = self.network_send {
            network_send.send(NetworkMessage::AddPeer(req.addr, tx)).await
                .map_err(|_| ProviderError::Internal("Network sender dropped".to_string()))?;
            rx.await.map_err(|_| ProviderError::Internal("Oneshot dropped".to_string()))?
                .map_err(|e| ProviderError::Internal(e))?;
            Ok(())
        } else {
            Err(ProviderError::Internal("Network handle not available".to_string()))
        }
    }

    async fn node_info(&self) -> Result<crate::rpc::evm_rpc::NetNodeInfoResponse, ProviderError> {
        let (tx, rx) = oneshot::channel();
        if let Some(ref network_send) = self.network_send {
            network_send.send(NetworkMessage::GetNodeInfo(tx)).await
                .map_err(|_| ProviderError::Internal("Network sender dropped".to_string()))?;
            let info = rx.await.map_err(|_| ProviderError::Internal("Oneshot dropped".to_string()))?;
            Ok(crate::rpc::evm_rpc::NetNodeInfoResponse {
                enr: info.enr,
                node_id: info.node_id,
                listen_addresses: info.listen_addresses,
            })
        } else {
            Err(ProviderError::Internal("Network handle not available".to_string()))
        }
    }
}

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::{Mutex, RwLock, oneshot, mpsc};
use alloy_primitives::{Address, B256, U256};

use crate::executor::Executor;
use crate::mempool::Mempool;
use crate::storage::storage::InMemoryStorage;
use crate::storage::types::{Block, Transaction, PendingPayload, Receipt, ExecutionPayload as DomainExecutionPayload};
use crate::rpc::provider_error::ProviderError;
use crate::rpc::provider_api::*;
use crate::network::NetworkMessage;

pub struct DefaultBlockchainProvider {
    pub storage: Arc<RwLock<InMemoryStorage>>,
    pub mempool: Arc<RwLock<Mempool>>,
    pub executor: Executor,
    pub network_send: Option<mpsc::Sender<NetworkMessage>>,
    pub tx_broadcast: Option<mpsc::Sender<Transaction>>,
    pub pending_payloads: Arc<Mutex<std::collections::HashMap<B256, PendingPayload>>>,
}

impl DefaultBlockchainProvider {
    pub fn new(
        storage: Arc<RwLock<InMemoryStorage>>,
        mempool: Arc<RwLock<Mempool>>,
        executor: Executor,
        network_send: Option<mpsc::Sender<NetworkMessage>>,
        tx_broadcast: Option<mpsc::Sender<Transaction>>,
        pending_payloads: Arc<Mutex<std::collections::HashMap<B256, PendingPayload>>>,
    ) -> Self {
        Self {
            storage,
            mempool,
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
        let storage = self.storage.read().await;
        Ok(storage.get_accounts())
    }

    async fn latest_block_number(&self) -> Result<u64, ProviderError> {
        let storage = self.storage.read().await;
        Ok(storage.get_latest_block_number())
    }

    async fn balance(&self, address: Address) -> Result<u128, ProviderError> {
        let storage = self.storage.read().await;
        Ok(storage.get_balance(address).to::<u128>())
    }

    async fn block_by_number(&self, number: u64) -> Result<Option<Block>, ProviderError> {
        let storage = self.storage.read().await;
        Ok(storage.get_block_by_number(number).cloned())
    }

    async fn block_by_hash(&self, hash: B256) -> Result<Option<Block>, ProviderError> {
        let storage = self.storage.read().await;
        Ok(storage.get_block_by_hash(hash).cloned())
    }

    async fn block_transaction_count_by_number(&self, number: u64) -> Result<u64, ProviderError> {
        let storage = self.storage.read().await;
        Ok(storage.get_block_by_number(number)
            .map(|b| b.body.execution_payload.transactions.len() as u64)
            .unwrap_or(0))
    }

    async fn block_transaction_count_by_hash(&self, hash: B256) -> Result<u64, ProviderError> {
        let storage = self.storage.read().await;
        Ok(storage.get_block_by_hash(hash)
            .map(|b| b.body.execution_payload.transactions.len() as u64)
            .unwrap_or(0))
    }

    async fn tx_by_hash(&self, hash: B256) -> Result<Option<Transaction>, ProviderError> {
        let storage = self.storage.read().await;
        Ok(storage.get_transaction_by_hash(hash).cloned())
    }

    async fn tx_receipt_by_hash(&self, hash: B256) -> Result<Option<(Transaction, Receipt, Block)>, ProviderError> {
        let storage = self.storage.read().await;
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
        let storage = self.storage.read().await;
        Ok(storage.get_code(address))
    }

    async fn roots(&self) -> Result<(B256, B256, Option<B256>), ProviderError> {
        let storage = self.storage.read().await;
        let latest_num = storage.get_latest_block_number();
        let latest_block = storage.get_block_by_number(latest_num);
        let receipts_root = latest_block.map(|b| b.body.execution_payload.receipts_root).unwrap_or(B256::ZERO);
        Ok((storage.calculate_state_root(), receipts_root, latest_block.map(|b| b.body.execution_payload.block_hash)))
    }

    async fn mempool(&self) -> Result<Vec<Transaction>, ProviderError> {
        let mempool = self.mempool.read().await;
        Ok(mempool.get_all_transactions())
    }
}

#[async_trait]
impl EthWriteProvider for DefaultBlockchainProvider {
    async fn send_transaction(&self, req: DomainTransactionRequest) -> Result<Transaction, ProviderError> {
        let tx = Transaction::builder(req.from)
            .nonce(req.nonce)
            .to(req.to)
            .value(req.value)
            .data(req.data)
            .gas_limit(req.gas_limit)
            .gas_price(req.gas_price)
            .build();

        // Broadcast to network if handle is available
        if let Some(ref tx_broadcast) = self.tx_broadcast {
            let _ = tx_broadcast.send(tx.clone()).await;
        }

        let mut mempool = self.mempool.write().await;
        mempool.add_transaction(tx.clone());
        Ok(tx)
    }

    async fn call(&self, req: DomainTransactionRequest) -> Result<Transaction, ProviderError> {
        let tx = Transaction::builder(req.from)
            .nonce(req.nonce)
            .to(req.to)
            .value(req.value)
            .data(req.data)
            .gas_limit(req.gas_limit)
            .gas_price(req.gas_price)
            .build();
        Ok(tx)
    }
}

#[async_trait]
impl EngineProvider for DefaultBlockchainProvider {
    async fn propose_block(&self, req: DomainProposeBlockRequest) -> Result<ProposeBlockResult, ProviderError> {
        let storage_arc = self.storage.clone();
        let mempool_arc = self.mempool.clone();
        let executor = self.executor.clone();
        let network_send = self.network_send.clone();

        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            
            let (mut storage, mut mempool) = rt.block_on(async {
                let s = storage_arc.write().await;
                let m = mempool_arc.write().await;
                (s, m)
            });

            let transactions = mempool.pop_transactions(100);

            let latest_block = storage.get_latest_block().cloned().expect("Genesis block should exist");
            
            let parent_hash = latest_block.body.execution_payload.block_hash;
            let timestamp = if req.timestamp == 0 {
                latest_block.body.execution_payload.timestamp + 12
            } else {
                req.timestamp
            };

            let block_builder = Block::builder(latest_block.body.execution_payload.block_number + 1)
                .parent_hash(parent_hash)
                .timestamp(timestamp)
                .fee_recipient(latest_block.body.execution_payload.fee_recipient);

            executor.execute_block(&mut storage, transactions.clone(), block_builder.build())
                .map_err(|e| ProviderError::Execution(e))?;

            // Get the actual block that was added to storage (with calculated roots)
            let block = storage.get_latest_block().cloned().ok_or_else(|| ProviderError::Internal("Block not found after execution".to_string()))?;
            let block_hash = block.body.execution_payload.block_hash;

            // Broadcast block
            if let Some(ref network_send) = network_send {
                let _ = rt.block_on(network_send.send(NetworkMessage::BroadcastBlock(block)));
            }

            Ok(ProposeBlockResult {
                block_hash,
                tx_results: transactions,
            })
        }).await.map_err(|e| ProviderError::Internal(format!("Task panicked: {}", e)))?
    }

    async fn engine_new_payload(&self, payload: DomainExecutionPayload) -> Result<DomainPayloadStatus, ProviderError> {
        let storage_arc = self.storage.clone();
        let executor = self.executor.clone();
        let network_send = self.network_send.clone();

        tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            let mut storage = rt.block_on(storage_arc.write());

            let block_hash = payload.block_hash;
            if storage.get_block_by_hash(block_hash).is_some() {
                return Ok(DomainPayloadStatus {
                    status: "VALID".to_string(),
                    latest_valid_hash: Some(block_hash),
                    validation_error: None,
                });
            }

            let transactions = payload.transactions.clone();
            let parent_hash = payload.parent_hash;

            let block = Block::builder(payload.block_number)
                .parent_hash(parent_hash)
                .timestamp(payload.timestamp)
                .fee_recipient(payload.fee_recipient)
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
                    if let Some(ref network_send) = network_send {
                        let _ = rt.block_on(network_send.send(NetworkMessage::BroadcastBlock(block)));
                    }

                    Ok(DomainPayloadStatus {
                        status: "VALID".to_string(),
                        latest_valid_hash: Some(block_hash),
                        validation_error: None,
                    })
                }
                Err(e) => {
                    Ok(DomainPayloadStatus {
                        status: "INVALID".to_string(),
                        latest_valid_hash: None,
                        validation_error: Some(e),
                    })
                }
            }
        }).await.map_err(|e| ProviderError::Internal(format!("Task panicked: {}", e)))?
    }

    async fn engine_forkchoice_updated(&self, req: DomainForkchoiceUpdatedRequest) -> Result<DomainForkchoiceUpdatedResponse, ProviderError> {
        let head_block_hash = req.head_block_hash;

        // 1. Update forkchoice in storage
        {
            let mut storage = self.storage.write().await;
            storage.update_forkchoice(head_block_hash);
        }

        // 2. Build payload if requested
        let payload_id = if let Some(attr) = req.payload_attributes {
            let storage_arc = self.storage.clone();
            let mempool_arc = self.mempool.clone();
            let executor = self.executor.clone();
            let pending_payloads_arc = self.pending_payloads.clone();

            tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                let (mut storage, mut mempool) = rt.block_on(async {
                    let s = storage_arc.read().await.clone();
                    let m = mempool_arc.read().await.clone();
                    (s, m)
                });

                let latest_block = storage.get_block_by_hash(head_block_hash)
                    .cloned()
                    .or_else(|| storage.get_latest_block().cloned())
                    .expect("Genesis block should exist");

                let transactions = mempool.peek_transactions(100);

                let block_builder = Block::builder(latest_block.body.execution_payload.block_number + 1)
                    .parent_hash(head_block_hash)
                    .timestamp(attr.timestamp)
                    .prev_randao(attr.prev_randao)
                    .fee_recipient(attr.suggested_fee_recipient);

                let dummy_block = block_builder.build();

                let (_results, receipts, total_changeset) = executor.execute_with_changeset(&mut storage, transactions.clone(), dummy_block.clone())
                    .map_err(|e| ProviderError::Execution(e))?;

                // Build finalized block with correct roots
                let mut block_builder = Block::builder(dummy_block.slot)
                    .parent_hash(dummy_block.body.execution_payload.parent_hash)
                    .timestamp(dummy_block.body.execution_payload.timestamp)
                    .prev_randao(dummy_block.body.execution_payload.prev_randao)
                    .fee_recipient(dummy_block.body.execution_payload.fee_recipient)
                    .transactions(transactions);

                for r in receipts.clone() {
                    block_builder = block_builder.add_receipt(r);
                }

                let block = block_builder.build();
                let id = block.body.execution_payload.block_hash;

                rt.block_on(async {
                    let mut pending = pending_payloads_arc.lock().await;
                    pending.insert(id, PendingPayload {
                        block,
                        receipts,
                        total_changeset,
                    });
                });

                Ok::<B256, ProviderError>(id)
            }).await.map_err(|e| ProviderError::Internal(format!("Task panicked: {}", e)))??
            .into()
        } else {
            None
        };

        Ok(DomainForkchoiceUpdatedResponse {
            status: "VALID".to_string(),
            payload_id,
        })
    }

    async fn engine_get_payload(&self, req: DomainGetPayloadRequest) -> Result<DomainExecutionPayload, ProviderError> {
        let mut pending = self.pending_payloads.lock().await;
        let payload = pending.remove(&req.payload_id).ok_or_else(|| ProviderError::NotFound("Payload not found"))?;
        Ok(payload.block.body.execution_payload)
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

    async fn peers(&self) -> Result<Vec<DomainPeerInfo>, ProviderError> {
        let (tx, rx) = oneshot::channel();
        if let Some(ref network_send) = self.network_send {
            network_send.send(NetworkMessage::GetPeers(tx)).await
                .map_err(|_| ProviderError::Internal("Network sender dropped".to_string()))?;
            let peers = rx.await.map_err(|_| ProviderError::Internal("Oneshot dropped".to_string()))?;
            Ok(peers.into_iter().map(|p| DomainPeerInfo {
                id: p.id,
                enode: p.addr,
                enr: p.enr.unwrap_or_default(),
                name: String::new(),
                caps: Vec::new(),
                network: DomainPeerNetworkInfo {
                    local_address: String::new(),
                    remote_address: String::new(),
                    inbound: false,
                    trusted: false,
                    static_node: false,
                },
                protocols: Vec::new(),
            }).collect())
        } else {
            Err(ProviderError::Internal("Network handle not available".to_string()))
        }
    }

    async fn add_peer(&self, req: DomainNetAddPeerRequest) -> Result<(), ProviderError> {
        let (tx, rx) = oneshot::channel();
        if let Some(ref network_send) = self.network_send {
            network_send.send(NetworkMessage::AddPeer(req.enode, tx)).await
                .map_err(|_| ProviderError::Internal("Network sender dropped".to_string()))?;
            rx.await.map_err(|_| ProviderError::Internal("Oneshot dropped".to_string()))?
                .map_err(|e| ProviderError::Internal(e))?;
            Ok(())
        } else {
            Err(ProviderError::Internal("Network handle not available".to_string()))
        }
    }

    async fn node_info(&self) -> Result<DomainNetNodeInfoResponse, ProviderError> {
        let (tx, rx) = oneshot::channel();
        if let Some(ref network_send) = self.network_send {
            network_send.send(NetworkMessage::GetNodeInfo(tx)).await
                .map_err(|_| ProviderError::Internal("Network sender dropped".to_string()))?;
            let info = rx.await.map_err(|_| ProviderError::Internal("Oneshot dropped".to_string()))?;
            Ok(DomainNetNodeInfoResponse {
                enode: info.node_id.clone(),
                enr: info.enr,
                name: String::new(),
                caps: Vec::new(),
                id: info.node_id,
                network: DomainNodeNetworkInfo {
                    local_address: String::new(),
                    remote_address: String::new(),
                    listen_addr: info.listen_addresses.first().cloned().unwrap_or_default(),
                },
                protocols: Vec::new(),
            })
        } else {
            Err(ProviderError::Internal("Network handle not available".to_string()))
        }
    }
}

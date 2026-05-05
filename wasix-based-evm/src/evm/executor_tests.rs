#[cfg(test)]
mod tests {
    use crate::evm::executor::Executor;
    use crate::storage::storage::InMemoryStorage;
    use crate::storage::traits::SyncStateProvider;
    use crate::evm::ev::alloy_u256_to_evm_u256;
    use alloy_primitives::{Address, U256, B256, TxKind, Bytes, FixedBytes};
    use alloy_consensus::{TxLegacy, TxEnvelope, Block, Header, BlockBody, SignableTransaction};
    use alloy_signer_local::PrivateKeySigner;
    use alloy_network::TxSignerSync;
    use evm_interpreter::uint::H160;

    fn apply_result(storage: &mut InMemoryStorage, result: crate::evm::executor::BlockExecutionResult) {
        storage.apply_changeset(&result.changeset);
        for (tx, receipt) in result.finalized_block.body.transactions.iter().zip(result.receipts.iter()) {
            storage.add_transaction(tx.clone());
            storage.add_receipt(*tx.hash(), receipt.clone());
        }
        for withdrawal in result.withdrawals {
            let addr = withdrawal.address;
            let amount_wei = U256::from(withdrawal.amount) * U256::from(1_000_000_000u64);
            let mut account = storage.get_account(addr).unwrap_or_else(|| evm::backend::InMemoryAccount {
                balance: crate::evm::ev::EvmU256::zero(),
                nonce: crate::evm::ev::EvmU256::zero(),
                code: Vec::new(),
                storage: std::collections::BTreeMap::new(),
                transient_storage: std::collections::BTreeMap::new(),
            });
            account.balance += crate::evm::ev::alloy_u256_to_evm_u256(amount_wei);
            storage.set_account(addr, account);
        }
        storage.add_block(result.finalized_block);
    }

    fn setup_executor() -> (Executor, InMemoryStorage, PrivateKeySigner, Address) {
        let chain_id = 1337u64;
        let signer = PrivateKeySigner::random();
        let addr = signer.address();

        let mut genesis = alloy_genesis::Genesis::default();
        genesis.config.chain_id = chain_id;
        genesis.alloc.insert(addr, alloy_genesis::GenesisAccount {
            balance: U256::from(10).pow(U256::from(25)),
            nonce: Some(0),
            ..Default::default()
        });

        let storage = InMemoryStorage::new_with_genesis(alloy_u256_to_evm_u256(U256::from(chain_id)), genesis);
        let executor = Executor::new();
        (executor, storage, signer, addr)
    }

    fn calculate_contract_address(caller: Address, nonce: u64) -> Address {
        let mut payload = Vec::new();
        payload.push(0x94);
        payload.extend_from_slice(caller.as_slice());
        if nonce == 0 {
            payload.push(0x80);
        } else if nonce < 0x80 {
            payload.push(nonce as u8);
        } else {
            payload.push(0x81);
            payload.push(nonce as u8);
        }
        let mut rlp = Vec::new();
        rlp.push(0xc0 + payload.len() as u8);
        rlp.extend(payload);
        let hash = alloy_primitives::keccak256(&rlp);
        Address::from_slice(&hash[12..])
    }

    #[tokio::test]
    async fn test_executor_transfer() {
        let (executor, mut storage, signer_a, addr_a) = setup_executor();
        let addr_b = Address::repeat_byte(0xbb);
        let chain_id = 1337u64;

        let mut tx = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 0,
            gas_limit: 21000,
            gas_price: 1,
            to: TxKind::Call(addr_b),
            value: U256::from(1000),
            input: Bytes::new(),
        };
        let signature = signer_a.sign_transaction_sync(&mut tx).unwrap();
        let signed_tx = tx.into_signed(signature);
        let envelope = TxEnvelope::Legacy(signed_tx);

        let block = Block {
            header: Header { number: 1, base_fee_per_gas: Some(0), ..Default::default() },
            body: BlockBody { transactions: vec![envelope.clone()], ..Default::default() },
        };

        let res = executor.execute_block(&mut storage, vec![envelope], block).expect("Execution failed");
        apply_result(&mut storage, res);

        assert_eq!(storage.get_balance(addr_b), U256::from(1000));
        assert_eq!(storage.get_balance(addr_a), U256::from(10).pow(U256::from(25)) - U256::from(22000));
    }

    #[tokio::test]
    async fn test_executor_deploy_and_call() {
        let (executor, mut storage, signer, addr) = setup_executor();
        let chain_id = 1337u64;

        let runtime = hex::decode("604260005560006000f3").unwrap();
        let mut init = hex::decode("600a80600b6000396000f3").unwrap();
        init.extend(&runtime);

        let mut tx_deploy = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 0,
            gas_limit: 1000000,
            gas_price: 1,
            to: TxKind::Create,
            value: U256::ZERO,
            input: init.into(),
        };
        let sig_deploy = signer.sign_transaction_sync(&mut tx_deploy).unwrap();
        let envelope_deploy = TxEnvelope::Legacy(tx_deploy.into_signed(sig_deploy));

        let block1 = Block {
            header: Header { number: 1, base_fee_per_gas: Some(0), ..Default::default() },
            body: BlockBody { transactions: vec![envelope_deploy.clone()], ..Default::default() },
        };

        let res1 = executor.execute_block(&mut storage, vec![envelope_deploy], block1).unwrap();
        apply_result(&mut storage, res1);
        let contract_addr = calculate_contract_address(addr, 0);
        let contract_addr_h160 = H160::from_slice(contract_addr.as_slice());
        println!("CODE: {:x?}", storage.get_code(contract_addr));
        assert!(!storage.get_code(contract_addr).is_empty());

        let mut tx_call = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 1,
            gas_limit: 1000000,
            gas_price: 1,
            to: TxKind::Call(contract_addr),
            value: U256::ZERO,
            input: Bytes::new(),
        };
        let sig_call = signer.sign_transaction_sync(&mut tx_call).unwrap();
        let envelope_call = TxEnvelope::Legacy(tx_call.into_signed(sig_call));

        let block2 = Block {
            header: Header { number: 2, base_fee_per_gas: Some(0), ..Default::default() },
            body: BlockBody { transactions: vec![envelope_call.clone()], ..Default::default() },
        };

        let res2 = executor.execute_block(&mut storage, vec![envelope_call], block2).unwrap();
        apply_result(&mut storage, res2);
        let account = storage.state.backend.state.get(&contract_addr_h160).expect("Account not found");
        println!("Account balance: {:?}", account.balance);
        println!("Account nonce: {:?}", account.nonce);
        println!("Account storage: {:?}", account.storage);
        
        let val = account.storage.get(&evm_interpreter::uint::H256::zero())
            .expect("Storage slot 0 not found in contract account after call");
        assert_eq!(U256::from_be_bytes(val.0), U256::from(0x42));
    }

    #[tokio::test]
    async fn test_executor_multiple_transactions_one_block() {
        let (executor, mut storage, signer_a, addr_a) = setup_executor();
        let chain_id = 1337u64;
        let addr_b = Address::repeat_byte(0xbb);
        let addr_c = Address::repeat_byte(0xcc);

        let mut tx1 = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 0,
            gas_limit: 21000,
            gas_price: 1,
            to: TxKind::Call(addr_b),
            value: U256::from(500),
            input: Bytes::new(),
        };
        let sig1 = signer_a.sign_transaction_sync(&mut tx1).unwrap();
        let env1 = TxEnvelope::Legacy(tx1.into_signed(sig1));

        let mut tx2 = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 1,
            gas_limit: 21000,
            gas_price: 1,
            to: TxKind::Call(addr_c),
            value: U256::from(300),
            input: Bytes::new(),
        };
        let sig2 = signer_a.sign_transaction_sync(&mut tx2).unwrap();
        let env2 = TxEnvelope::Legacy(tx2.into_signed(sig2));

        let block = Block {
            header: Header { number: 1, base_fee_per_gas: Some(0), ..Default::default() },
            body: BlockBody { transactions: vec![env1.clone(), env2.clone()], ..Default::default() },
        };

        let res = executor.execute_block(&mut storage, vec![env1, env2], block).unwrap();
        apply_result(&mut storage, res);

        assert_eq!(storage.get_balance(addr_b), U256::from(500));
        assert_eq!(storage.get_balance(addr_c), U256::from(300));
        assert_eq!(storage.get_balance(addr_a), U256::from(10).pow(U256::from(25)) - U256::from(42800));
    }
    #[tokio::test]
    async fn test_executor_out_of_gas() {
        let (executor, mut storage, signer, _addr) = setup_executor();
        let chain_id = 1337u64;
        let addr_b = Address::repeat_byte(0xcc);

        let mut tx = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 0,
            gas_limit: 100, // Very low gas limit
            gas_price: 0,
            to: TxKind::Call(addr_b),
            value: U256::from(1000),
            input: Bytes::new(),
        };
        let signature = signer.sign_transaction_sync(&mut tx).unwrap();
        let signed_tx = tx.into_signed(signature);
        let envelope = TxEnvelope::Legacy(signed_tx);

        let block = Block {
            header: Header { number: 1, base_fee_per_gas: Some(0), ..Default::default() },
            body: BlockBody { transactions: vec![envelope.clone()], ..Default::default() },
        };

        let res = executor.execute_block(&mut storage, vec![envelope], block);
        if let Ok(ref result) = res {
            apply_result(&mut storage, result.clone());
        }
        // If it returns Err, it's because transact() returned Err.
        assert!(res.is_err() || storage.get_balance(addr_b) == U256::ZERO);
    }

    #[tokio::test]
    async fn test_executor_revert() {
        let (executor, mut storage, signer, addr) = setup_executor();
        let chain_id = 1337u64;

        let runtime = hex::decode("60006000fd").unwrap();
        let mut init = hex::decode("600580600c6000396000f3").unwrap();
        init.extend(&runtime);

        let mut tx_deploy = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 0,
            gas_limit: 1000000,
            gas_price: 1,
            to: TxKind::Create,
            value: U256::ZERO,
            input: init.into(),
        };
        let sig_deploy = signer.sign_transaction_sync(&mut tx_deploy).unwrap();
        let envelope_deploy = TxEnvelope::Legacy(tx_deploy.into_signed(sig_deploy));

        let block1 = Block {
            header: Header { number: 1, ..Default::default() },
            body: BlockBody { transactions: vec![envelope_deploy.clone()], ..Default::default() },
        };

        let res1 = executor.execute_block(&mut storage, vec![envelope_deploy], block1).unwrap();
        apply_result(&mut storage, res1);
        let contract_addr = addr.create(0);

        let mut tx_call = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 1,
            gas_limit: 1000000,
            gas_price: 1,
            to: TxKind::Call(contract_addr),
            value: U256::ZERO,
            input: Bytes::new(),
        };
        let sig_call = signer.sign_transaction_sync(&mut tx_call).unwrap();
        let envelope_call = TxEnvelope::Legacy(tx_call.into_signed(sig_call));

        let block2 = Block {
            header: Header { number: 2, ..Default::default() },
            body: BlockBody { transactions: vec![envelope_call.clone()], ..Default::default() },
        };

        let res2 = executor.execute_block(&mut storage, vec![envelope_call], block2).unwrap();
        apply_result(&mut storage, res2.clone());
        assert_eq!(res2.results.len(), 1);
    }

    #[tokio::test]
    async fn test_executor_multiple_transactions() {
        let (executor, mut storage, signer_a, _addr_a) = setup_executor();
        let signer_b = PrivateKeySigner::random();
        let addr_b = signer_b.address();
        let chain_id = 1337u64;

        {
            storage.set_balance(addr_b, U256::from(10).pow(U256::from(20)));
        }

        let addr_c = Address::repeat_byte(0xdd);

        let mut tx1 = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 0,
            gas_limit: 21000,
            gas_price: 1,
            to: TxKind::Call(addr_c),
            value: U256::from(500),
            input: Bytes::new(),
        };
        let sig1 = signer_a.sign_transaction_sync(&mut tx1).unwrap();
        let env1 = TxEnvelope::Legacy(tx1.into_signed(sig1));

        let mut tx2 = TxLegacy {
            chain_id: Some(chain_id),
            nonce: 0,
            gas_limit: 21000,
            gas_price: 1,
            to: TxKind::Call(addr_c),
            value: U256::from(300),
            input: Bytes::new(),
        };
        let sig2 = signer_b.sign_transaction_sync(&mut tx2).unwrap();
        let env2 = TxEnvelope::Legacy(tx2.into_signed(sig2));

        let block = Block {
            header: Header { number: 1, ..Default::default() },
            body: BlockBody { transactions: vec![env1.clone(), env2.clone()], ..Default::default() },
        };

        let res = executor.execute_block(&mut storage, vec![env1, env2], block).unwrap();
        apply_result(&mut storage, res);

        assert_eq!(storage.get_balance(addr_c), U256::from(800));
    }
}

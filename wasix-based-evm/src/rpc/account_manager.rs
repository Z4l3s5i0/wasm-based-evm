use alloy_primitives::{Address, Bytes, B256};
use alloy_consensus::{TxEnvelope, TxLegacy, SignableTransaction};
use alloy_signer_local::PrivateKeySigner;
use alloy_network::TxSignerSync;
use alloy_signer::SignerSync;
use std::collections::HashMap;
use crate::error::{RpcResult, RpcError};

/// Manages accounts and their private keys for node-side signing.
pub struct AccountManager {
    /// Mapping of address to its private key signer.
    pub signers: HashMap<Address, PrivateKeySigner>,
}

impl AccountManager {
    /// Create a new empty AccountManager.
    pub fn new() -> Self {
        Self {
            signers: HashMap::new(),
        }
    }

    /// Create a new AccountManager with default developer keys (standard Ganache/Hardhat addresses).
    pub fn new_with_dev_keys() -> Self {
        let mut manager = Self::new();
        
        // Standard Hardhat/Ganache dev keys (private keys are public knowledge)
        let dev_keys = [
            "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80", // 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
            "59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d", // 0x70997970C51812dc3A010C7d01b50e0d17dc79C8
            "5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a", // 0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC
        ];

        for key_hex in dev_keys {
            if let Ok(signer) = key_hex.parse::<PrivateKeySigner>() {
                manager.add_signer(signer);
            }
        }

        manager
    }

    /// Add a new signer to the manager.
    pub fn add_signer(&mut self, signer: PrivateKeySigner) {
        self.signers.insert(signer.address(), signer);
    }

    /// Check if an address is managed by this node.
    pub fn is_managed(&self, address: &Address) -> bool {
        self.signers.contains_key(address)
    }

    /// Get all managed addresses.
    pub fn managed_addresses(&self) -> Vec<Address> {
        self.signers.keys().cloned().collect()
    }

    /// Sign a transaction for a managed address.
    pub async fn sign_transaction(&self, from: &Address, mut tx: TxLegacy) -> RpcResult<TxEnvelope> {
        let signer = self.signers.get(from)
            .ok_or_else(|| RpcError::AccountNotFound(*from))?;

        // In a real implementation, we should support EIP-1559 and EIP-2930 as well.
        // For now, let's focus on Legacy signing.
        let signature = signer.sign_transaction_sync(&mut tx)
            .map_err(|e| RpcError::Internal(format!("Signing failed: {}", e)))?;

        // Create the signed transaction envelope.
        let signed_tx = tx.into_signed(signature);
        Ok(TxEnvelope::Legacy(signed_tx))
    }

    /// Sign a message for a managed address.
    pub async fn sign(&self, from: &Address, message: &[u8]) -> RpcResult<alloy_primitives::Signature> {
        let signer = self.signers.get(from)
            .ok_or_else(|| RpcError::AccountNotFound(*from))?;

        let signature = signer.sign_message_sync(message)
            .map_err(|e| RpcError::Internal(format!("Signing failed: {}", e)))?;

        Ok(signature)
    }
}

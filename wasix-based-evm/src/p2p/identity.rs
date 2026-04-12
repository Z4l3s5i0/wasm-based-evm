use libp2p::identity::secp256k1;
use std::fs;
use std::path::Path;
use anyhow::{Context, Result};

pub struct Identity {
    pub keypair: libp2p::identity::Keypair,
}

impl Identity {
    pub fn new(
        data_dir: Option<&Path>,
    ) -> Result<Self> {
        let keypair = match data_dir {
            Some(path) => {
                let key_path = path.join("p2p_key");
                if key_path.exists() {
                    let hex_key = fs::read_to_string(&key_path)
                        .context("Failed to read p2p_key file")?;
                    let bytes = hex::decode(hex_key.trim())
                        .context("Failed to decode hex p2p_key")?;
                    let secret = secp256k1::SecretKey::try_from_bytes(bytes)
                        .map_err(|_| anyhow::anyhow!("Invalid p2p_key bytes"))?;
                    libp2p::identity::Keypair::from(secp256k1::Keypair::from(secret))
                } else {
                    use rand::RngCore;
                    let mut bytes = [0u8; 32];
                    rand::thread_rng().fill_bytes(&mut bytes);
                    let secret = secp256k1::SecretKey::try_from_bytes(bytes)
                        .expect("32 bytes is valid secret key length");
                    let hex_key = hex::encode(secret.to_bytes());
                    if let Some(parent) = key_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&key_path, hex_key)?;
                    libp2p::identity::Keypair::from(secp256k1::Keypair::from(secret))
                }
            }
            None => {
                use rand::RngCore;
                let mut bytes = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut bytes);
                let secret = secp256k1::SecretKey::try_from_bytes(bytes)
                    .expect("32 bytes is valid secret key length");
                libp2p::identity::Keypair::from(secp256k1::Keypair::from(secret))
            }
        };

        Ok(Self { keypair })
    }
}

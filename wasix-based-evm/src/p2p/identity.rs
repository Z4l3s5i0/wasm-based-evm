use discv5::enr::{CombinedKey, Enr, Builder as EnrBuilder};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use anyhow::{Context, Result};

pub struct Identity {
    pub keypair: CombinedKey,
    pub enr: Enr<CombinedKey>,
}

impl Identity {
    pub fn new(
        data_dir: Option<&Path>,
        p2p_port: u16,
        discovery_port: u16,
        ext_ip: Option<IpAddr>,
    ) -> Result<Self> {
        let keypair = match data_dir {
            Some(path) => {
                let key_path = path.join("p2p_key");
                if key_path.exists() {
                    let hex_key = fs::read_to_string(&key_path)
                        .context("Failed to read p2p_key file")?;
                    let bytes = hex::decode(hex_key.trim())
                        .context("Failed to decode hex p2p_key")?;
                    let secret = discv5::enr::k256::ecdsa::SigningKey::from_slice(&bytes)
                        .map_err(|_| anyhow::anyhow!("Invalid p2p_key bytes"))?;
                    CombinedKey::from(secret)
                } else {
                    let secret = discv5::enr::k256::ecdsa::SigningKey::from_slice(&rand::random::<[u8; 32]>())
                         .expect("32 bytes is valid secret key length");
                    let hex_key = hex::encode(secret.to_bytes());
                    fs::create_dir_all(path)?;
                    fs::write(&key_path, hex_key)?;
                    CombinedKey::from(secret)
                }
            }
            None => {
                let secret = discv5::enr::k256::ecdsa::SigningKey::from_slice(&rand::random::<[u8; 32]>())
                    .expect("32 bytes is valid secret key length");
                CombinedKey::from(secret)
            }
        };

        let mut builder = EnrBuilder::default();
        if let Some(ip) = ext_ip {
            match ip {
                IpAddr::V4(ip4) => {
                    builder.ip4(ip4);
                    builder.udp4(discovery_port);
                    builder.tcp4(p2p_port);
                }
                IpAddr::V6(ip6) => {
                    builder.ip6(ip6);
                    builder.udp6(discovery_port);
                    builder.tcp6(p2p_port);
                }
            }
        } else {
            // Default to IPv4 localhost if no external IP provided for local testing
            builder.ip4(std::net::Ipv4Addr::LOCALHOST);
            builder.udp4(discovery_port);
            builder.tcp4(p2p_port);
        }

        let enr = builder.build(&keypair)
            .map_err(|e| anyhow::anyhow!("Failed to build ENR: {}", e))?;

        Ok(Self { keypair, enr })
    }
}

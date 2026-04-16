use std::fs;
use std::path::Path;
use anyhow::{Context, Result};
use k256::ecdsa::SigningKey;
use rcgen::{CertificateParams, DistinguishedName, KeyPair};

#[derive(Clone)]
pub struct Identity {
    pub keypair: SigningKey,
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
                    SigningKey::from_slice(&bytes)
                        .map_err(|_| anyhow::anyhow!("Invalid p2p_key bytes"))?
                } else {
                    let mut bytes = [0u8; 32];
                    getrandom::getrandom(&mut bytes)
                        .map_err(|e| anyhow::anyhow!("getrandom failed: {:?}", e))?;
                    let secret = SigningKey::from_slice(&bytes)
                        .expect("32 bytes is valid secret key length");
                    let hex_key = hex::encode(secret.to_bytes());
                    if let Some(parent) = key_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&key_path, hex_key)?;
                    secret
                }
            }
            None => {
                let mut bytes = [0u8; 32];
                getrandom::getrandom(&mut bytes)
                    .map_err(|e| anyhow::anyhow!("getrandom failed: {:?}", e))?;
                SigningKey::from_slice(&bytes)
                    .expect("32 bytes is valid secret key length")
            }
        };

        Ok(Self { keypair })
    }

    /// Get the PeerId as a hex string (from public key)
    pub fn peer_id(&self) -> String {
        let pubkey = self.keypair.verifying_key();
        hex::encode(pubkey.to_sec1_bytes())
    }

    pub fn generate_tls_config(&self, ext_ip: Option<std::net::IpAddr>) -> Result<(Vec<u8>, Vec<u8>)> {
        let keypair = KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|e| anyhow::anyhow!("Failed to generate rcgen KeyPair: {}", e))?;

        let mut params = CertificateParams::default();
        params.distinguished_name = DistinguishedName::new();
        params.distinguished_name.push(rcgen::DnType::CommonName, "wasix-p2p");
        // We'll also add some standard cert extensions that might be expected.
        params.not_before = rcgen::date_time_ymd(2020, 1, 1);
        params.not_after = rcgen::date_time_ymd(2030, 1, 1);
        params.subject_alt_names = vec![
            rcgen::SanType::DnsName("localhost".to_string().parse()?),
            rcgen::SanType::IpAddress("127.0.0.1".parse()?),
            rcgen::SanType::IpAddress("0.0.0.0".parse()?),
        ];
        if let Some(ip) = ext_ip {
            params.subject_alt_names.push(rcgen::SanType::IpAddress(ip));
        }
        
        let cert = params.self_signed(&keypair)
            .map_err(|e| anyhow::anyhow!("Failed to generate certificate: {}", e))?;
        
        let cert_der = cert.der().to_vec();
        let key_der = keypair.serialize_der();

        Ok((cert_der, key_der))
    }
}

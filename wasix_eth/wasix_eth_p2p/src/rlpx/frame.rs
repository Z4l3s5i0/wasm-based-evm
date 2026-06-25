use aes::cipher::{BlockEncrypt, KeyInit};
use aes::{Aes256, Aes256Enc};
use anyhow::{anyhow, Result};
use sha3::{Digest, Keccak256};
use alloy_primitives::{B128, B256};

pub struct MAC {
    secret: B256,
    hasher: Keccak256,
}

impl MAC {
    pub fn new(secret: B256) -> Self {
        Self { secret, hasher: Keccak256::new() }
    }

    pub fn update(&mut self, data: &[u8]) {
        self.hasher.update(data)
    }

    pub fn update_header(&mut self, data: &[u8; 16]) {
        let aes = Aes256Enc::new(self.secret.as_slice().into());
        let mut block = [0u8; 16];
        block.copy_from_slice(self.digest().as_slice());
        
        aes.encrypt_block(
            aes::cipher::generic_array::GenericArray::from_mut_slice(&mut block)
        );
        for i in 0..16 {
            block[i] ^= data[i];
        }
        self.hasher.update(block);
    }

    pub fn update_body(&mut self, data: &[u8]) {
        self.hasher.update(data);
        let prev = self.digest();
        let aes = Aes256Enc::new(self.secret.as_slice().into());
        let mut block = [0u8; 16];
        block.copy_from_slice(prev.as_slice());

        aes.encrypt_block(
            aes::cipher::generic_array::GenericArray::from_mut_slice(&mut block)
        );
        for i in 0..16 {
            block[i] ^= prev[i];
        }
        self.hasher.update(block);
    }

    pub fn digest(&self) -> B128 {
        B128::from_slice(&self.hasher.clone().finalize()[..16])
    }
}

pub struct RlpxFrameCodec {
    pub enc: ctr::Ctr64BE<Aes256>,
    pub dec: ctr::Ctr64BE<Aes256>,
    pub egress_mac: MAC,
    pub ingress_mac: MAC,
}

impl RlpxFrameCodec {
    pub fn new(secrets: &crate::rlpx::crypto::HandshakeSecrets) -> Self {
        use aes::cipher::KeyIvInit;
        let iv = [0u8; 16];
        
        Self {
            enc: ctr::Ctr64BE::<Aes256>::new(secrets.aes_secret.as_slice().into(), &iv.into()),
            dec: ctr::Ctr64BE::<Aes256>::new(secrets.aes_secret.as_slice().into(), &iv.into()),
            egress_mac: {
                let mut mac = MAC::new(secrets.mac_secret);
                mac.update(secrets.egress_mac_init_data.as_slice());
                mac
            },
            ingress_mac: {
                let mut mac = MAC::new(secrets.mac_secret);
                mac.update(secrets.ingress_mac_init_data.as_slice());
                mac
            },
        }
    }

    pub fn write_frame(&mut self, msg_id: u8, payload: &[u8]) -> Vec<u8> {
        use aes::cipher::StreamCipher;
        let mut rlp_id = Vec::new();
        alloy_rlp::Encodable::encode(&msg_id, &mut rlp_id);
        
        let frame_size = rlp_id.len() + payload.len();
        let mut header = [0u8; 16];
        header[0..3].copy_from_slice(&(frame_size as u32).to_be_bytes()[1..4]);
        
        // Properly RLP-encode frame header data: RLP([capability-id, context-id])
        // For p2p/eth it is usually [0, 0] which is [0xc2, 0x80, 0x80] in RLP.
        // EIP-8 specifies that this should be RLP([capability-id, context-id, ...])
        let header_data = [0xc2, 0x80, 0x80];
        header[3..3+header_data.len()].copy_from_slice(&header_data);
        
        let mut header_ciphertext = header;
        self.enc.apply_keystream(&mut header_ciphertext);
        
        self.egress_mac.update_header(&header_ciphertext);
        let header_mac = self.egress_mac.digest();
        
        let mut frame_ciphertext = Vec::with_capacity(frame_size);
        frame_ciphertext.extend_from_slice(&rlp_id);
        frame_ciphertext.extend_from_slice(payload);
        
        // Padding to 16 bytes
        let pad_len = (16 - (frame_size % 16)) % 16;
        if pad_len > 0 {
            frame_ciphertext.extend(std::iter::repeat(0).take(pad_len));
        }
        
        self.enc.apply_keystream(&mut frame_ciphertext);
        
        let mut result = Vec::with_capacity(32 + frame_ciphertext.len() + 16);
        result.extend_from_slice(&header_ciphertext);
        result.extend_from_slice(header_mac.as_slice());
        result.extend_from_slice(&frame_ciphertext);
        
        self.egress_mac.update_body(&frame_ciphertext);
        let frame_mac = self.egress_mac.digest();
        result.extend_from_slice(frame_mac.as_slice());
        
        result
    }

    pub fn read_header(&mut self, header_ciphertext: &[u8; 16], header_mac: &[u8; 16]) -> Result<usize> {
        use aes::cipher::StreamCipher;
        
        self.ingress_mac.update_header(header_ciphertext);
        let expected_mac = self.ingress_mac.digest();
        
        if expected_mac.as_slice() != header_mac {
            return Err(crate::error::P2pError::HeaderMacMismatch.into());
        }
        
        let mut header = *header_ciphertext;
        self.dec.apply_keystream(&mut header);
        
        let frame_size = (u32::from_be_bytes([0, header[0], header[1], header[2]])) as usize;
        
        if frame_size > 16 * 1024 * 1024 {
            return Err(anyhow!("Frame size too large: {}", frame_size));
        }

        Ok(frame_size)
    }

    pub fn read_frame_payload(&mut self, frame_ciphertext: &[u8], frame_mac: &[u8; 16]) -> Result<Vec<u8>> {
        use aes::cipher::StreamCipher;
        
        self.ingress_mac.update_body(frame_ciphertext);
        let expected_mac = self.ingress_mac.digest();

        if expected_mac.as_slice() != frame_mac {
            return Err(crate::error::P2pError::FrameMacMismatch.into());
        }
        
        let mut payload = frame_ciphertext.to_vec();
        self.dec.apply_keystream(&mut payload);
        
        Ok(payload)
    }
}

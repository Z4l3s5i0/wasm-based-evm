use alloy_primitives::{B256, B512, keccak256};
use alloy_rlp::Decodable;
use k256::ecdsa::{SigningKey, Signature, VerifyingKey};
use crate::discovery::v4::message::{Ping, Pong, FindNode, Neighbors, ENRRequest, ENRResponse};

#[derive(Debug)]
pub enum DecodeError {
    TooShort,
    HashMismatch,
    InvalidSignature(String),
    InvalidPacketType(u8),
    RlpError(alloy_rlp::Error),
    Expired,
    InvalidIP,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::TooShort => write!(f, "packet too short"),
            DecodeError::HashMismatch => write!(f, "hash mismatch"),
            DecodeError::InvalidSignature(e) => write!(f, "signature verification failed: {}", e),
            DecodeError::InvalidPacketType(t) => write!(f, "unknown packet type {}", t),
            DecodeError::RlpError(e) => write!(f, "invalid RLP payload: {}", e),
            DecodeError::Expired => write!(f, "expired packet ignored"),
            DecodeError::InvalidIP => write!(f, "invalid IP address length"),
        }
    }
}

impl From<alloy_rlp::Error> for DecodeError {
    fn from(e: alloy_rlp::Error) -> Self {
        DecodeError::RlpError(e)
    }
}

pub enum Packet {
    Ping(Ping),
    Pong(Pong),
    FindNode(FindNode),
    Neighbors(Neighbors),
    ENRRequest(ENRRequest),
    ENRResponse(ENRResponse),
}

impl Packet {
    pub fn packet_type(&self) -> u8 {
        match self {
            Packet::Ping(_) => 0x01,
            Packet::Pong(_) => 0x02,
            Packet::FindNode(_) => 0x03,
            Packet::Neighbors(_) => 0x04,
            Packet::ENRRequest(_) => 0x05,
            Packet::ENRResponse(_) => 0x06,
        }
    }

    pub fn encode_data(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            Packet::Ping(p) => alloy_rlp::Encodable::encode(p, &mut out),
            Packet::Pong(p) => alloy_rlp::Encodable::encode(p, &mut out),
            Packet::FindNode(p) => alloy_rlp::Encodable::encode(p, &mut out),
            Packet::Neighbors(p) => alloy_rlp::Encodable::encode(p, &mut out),
            Packet::ENRRequest(p) => alloy_rlp::Encodable::encode(p, &mut out),
            Packet::ENRResponse(p) => alloy_rlp::Encodable::encode(p, &mut out),
        }
        out
    }

    pub fn decode_payload(packet_type: u8, mut buf: &[u8]) -> Result<Self, DecodeError> {
        match packet_type {
            0x01 => Ok(Packet::Ping(Decodable::decode(&mut buf)?)),
            0x02 => Ok(Packet::Pong(Decodable::decode(&mut buf)?)),
            0x03 => Ok(Packet::FindNode(Decodable::decode(&mut buf)?)),
            0x04 => Ok(Packet::Neighbors(Decodable::decode(&mut buf)?)),
            0x05 => Ok(Packet::ENRRequest(Decodable::decode(&mut buf)?)),
            0x06 => Ok(Packet::ENRResponse(Decodable::decode(&mut buf)?)),
            _ => Err(DecodeError::InvalidPacketType(packet_type)),
        }
    }
}

pub struct RawPacket {
    pub hash: B256,
    pub signature: [u8; 65],
    pub packet_type: u8,
    pub data: Vec<u8>,
}

impl RawPacket {
    pub fn new(packet: Packet, key: &SigningKey) -> Self {
        let packet_type = packet.packet_type();
        let data = packet.encode_data();
        
        let mut sig_data = Vec::with_capacity(data.len() + 1);
        sig_data.push(packet_type);
        sig_data.extend_from_slice(&data);
        
        let (signature, recovery_id) = key.sign_prehash_recoverable(&keccak256(&sig_data).0).expect("Sign failed");
        let mut sig_bytes = [0u8; 65];
        sig_bytes[..64].copy_from_slice(&signature.to_bytes());
        sig_bytes[64] = recovery_id.to_byte();

        let mut hash_data = Vec::with_capacity(65 + sig_data.len());
        hash_data.extend_from_slice(&sig_bytes);
        hash_data.extend_from_slice(&sig_data);
        let hash = keccak256(&hash_data);

        RawPacket {
            hash,
            signature: sig_bytes,
            packet_type,
            data,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + 65 + 1 + self.data.len());
        out.extend_from_slice(&self.hash.0);
        out.extend_from_slice(&self.signature);
        out.push(self.packet_type);
        out.extend_from_slice(&self.data);
        out
    }

    pub fn decode(buf: &[u8]) -> Result<Self, DecodeError> {
        // STEP 1: size check
        if buf.len() < 98 {
            return Err(DecodeError::TooShort);
        }

        // STEP 2: extract fields safely
        let hash = B256::from_slice(&buf[0..32]);
        let signature: [u8; 65] = buf[32..97].try_into().unwrap();
        let packet_type = buf[97];

        // STEP 3: validate packet type early
        if packet_type < 0x01 || packet_type > 0x06 {
            return Err(DecodeError::InvalidPacketType(packet_type));
        }

        // STEP 4: verify hash BEFORE parsing payload
        let expected_hash = keccak256(&buf[32..]);
        if hash != expected_hash {
            return Err(DecodeError::HashMismatch);
        }

        // STEP 5: isolate payload safely
        let data = buf[98..].to_vec();

        Ok(RawPacket {
            hash,
            signature,
            packet_type,
            data,
        })
    }

    pub fn recover_public_key(&self) -> Result<B512, DecodeError> {
        let mut sig_data = Vec::with_capacity(self.data.len() + 1);
        sig_data.push(self.packet_type);
        sig_data.extend_from_slice(&self.data);
        let msg_hash = keccak256(&sig_data);

        let recovery_id = k256::ecdsa::RecoveryId::from_byte(self.signature[64])
            .ok_or_else(|| DecodeError::InvalidSignature("Invalid recovery ID".to_string()))?;
        let signature = Signature::from_slice(&self.signature[..64])
            .map_err(|e| DecodeError::InvalidSignature(e.to_string()))?;
        
        let key = VerifyingKey::recover_from_prehash(&msg_hash.0, &signature, recovery_id)
            .map_err(|e| DecodeError::InvalidSignature(e.to_string()))?;
        
        let uncompressed = key.to_encoded_point(false);
        let mut id_bytes = [0u8; 64];
        id_bytes.copy_from_slice(&uncompressed.as_bytes()[1..]);
        Ok(B512::from(id_bytes))
    }
}

use crate::error::P2pError;
use std::collections::VecDeque;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use crate::rlpx::frame::RlpxFrameCodec;
use crate::rlpx::crypto::HandshakeSecrets;
use alloy_rlp::Decodable;
use anyhow::anyhow;
use alloy_rlp::Encodable;
use anyhow::Result;
use alloy_primitives::B512;
use k256::SecretKey;
use wasix_eth_types::p2p::EthVersion;

pub struct SharedCapability {
    pub name: String,
    pub version: u8,
    pub offset: u8,
}

pub struct RlpxStream<S> {
    pub(crate) inner: S,
    pub remote_id: Option<B512>,
    pub remote_client_version: Option<String>,
    pub secrets: Option<HandshakeSecrets>,
    pub codec: Option<RlpxFrameCodec>,
    pub shared_capabilities: Vec<SharedCapability>,
    pub msg_buffer: VecDeque<(u8, Vec<u8>)>,
    pub initiator: bool,
}

impl RlpxStream<TcpStream> {
    pub async fn connect(addr: &str, _local_sk: &SecretKey, remote_id: &B512) -> Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        Ok(Self { 
            inner: stream, 
            remote_id: Some(*remote_id), 
            remote_client_version: None, 
            secrets: None, 
            codec: None,
            shared_capabilities: Vec::new(),
            msg_buffer: VecDeque::new(),
            initiator: true,
        })
    }

    pub async fn accept(stream: TcpStream) -> Result<Self> {
        Ok(Self { 
            inner: stream, 
            remote_id: None, 
            remote_client_version: None, 
            secrets: None, 
            codec: None,
            shared_capabilities: Vec::new(),
            msg_buffer: VecDeque::new(),
            initiator: false,
        })
    }
}

impl<S: AsyncReadExt + AsyncWriteExt + Unpin> RlpxStream<S> {
    pub async fn send_p2p<M: Encodable>(&mut self, msg: &M, id: u8) -> Result<()> {
        let mut payload = Vec::new();
        msg.encode(&mut payload);
        
        // EIP-706: Hello(0x00) and Disconnect(0x01) are never compressed.
        // P2P messages 0x02-0x0f ARE compressed if snappy is enabled.
        let final_payload = if self.snappy_enabled() && id > 1 {
            snap::raw::Encoder::new().compress_vec(&payload)?
        } else {
            payload
        };
        self.send_raw_payload(id, &final_payload).await
    }

    pub fn snappy_enabled(&self) -> bool {
        self.shared_capabilities.iter().any(|c| (c.name == "eth" && c.version >= 65) || c.name == "snap")
    }

    pub async fn send_eth<M: Encodable>(&mut self, msg: &M, id: u8) -> Result<()> {
        let eth_cap = self.shared_capabilities.iter().find(|c| c.name == "eth")
            .ok_or_else(|| anyhow!("Eth capability not negotiated"))?;
        
        let mut payload = Vec::new();
        msg.encode(&mut payload);
        
        let final_payload = if self.snappy_enabled() {
            snap::raw::Encoder::new().compress_vec(&payload)?
        } else {
            payload
        };
        self.send_raw_payload(id + eth_cap.offset, &final_payload).await
    }

    pub fn eth_version(&self) -> Option<EthVersion> {
        self.shared_capabilities.iter()
            .find(|c| c.name == "eth")
            .and_then(|c| EthVersion::try_from(c.version).ok())
    }

    pub async fn send_eth_raw(&mut self, id: u8, payload: &[u8]) -> Result<()> {
        let eth_cap = self.shared_capabilities.iter().find(|c| c.name == "eth")
            .ok_or_else(|| anyhow!("Eth capability not negotiated"))?;
        
        let final_payload = if self.snappy_enabled() {
            snap::raw::Encoder::new().compress_vec(payload)?
        } else {
            payload.to_vec()
        };
        self.send_raw_payload(id + eth_cap.offset, &final_payload).await
    }

    pub async fn send_snap<M: Encodable>(&mut self, msg: &M, id: u8) -> Result<()> {
        let snap_cap = self.shared_capabilities.iter().find(|c| c.name == "snap")
            .ok_or_else(|| anyhow!("Snap capability not negotiated"))?;
        
        let mut payload = Vec::new();
        msg.encode(&mut payload);
        
        // EIP-2364: All snap messages are snappy compressed
        let compressed = snap::raw::Encoder::new().compress_vec(&payload)?;
        self.send_raw_payload(id + snap_cap.offset, &compressed).await
    }

    pub async fn send_snap_raw(&mut self, id: u8, payload: &[u8]) -> Result<()> {
        let snap_cap = self.shared_capabilities.iter().find(|c| c.name == "snap")
            .ok_or_else(|| anyhow!("Snap capability not negotiated"))?;
        
        // EIP-2364: All snap messages are snappy compressed
        let compressed = snap::raw::Encoder::new().compress_vec(payload)?;
        self.send_raw_payload(id + snap_cap.offset, &compressed).await
    }

    async fn send_raw_payload(&mut self, id: u8, payload: &[u8]) -> Result<()> {
        let codec = self.codec.as_mut().ok_or_else(|| anyhow!("Codec not initialized"))?;
        let frame = codec.write_frame(id, payload);
        
        wasix_eth_utils::metrics::P2P_MESSAGES_SENT_BYTES.inc_by(frame.len() as f64);
        self.inner.write_all(&frame).await?;
        Ok(())
    }

    pub async fn read_message(&mut self) -> Result<(u8, Vec<u8>)> {
        if let Some(msg) = self.msg_buffer.pop_front() {
            return Ok(msg);
        }
        let codec = self.codec.as_mut().ok_or_else(|| anyhow!("Codec not initialized"))?;
        
        let mut header_ciphertext = [0u8; 16];
        self.inner.read_exact(&mut header_ciphertext).await?;
        let mut header_mac = [0u8; 16];
        self.inner.read_exact(&mut header_mac).await?;
        
        let frame_size = codec.read_header(&header_ciphertext, &header_mac)?;
        
        // reth: MAX_PAYLOAD_SIZE = 16MB. FrameCodec already checks for 16MB.
        const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;
        
        let padded_size = (frame_size + 15) / 16 * 16;
        let mut frame_ciphertext = vec![0u8; padded_size];
        self.inner.read_exact(&mut frame_ciphertext).await?;
        
        let mut frame_mac = [0u8; 16];
        self.inner.read_exact(&mut frame_mac).await?;
        
        let decrypted = codec.read_frame_payload(&frame_ciphertext, &frame_mac)?;
        
        // Truncate to frame_size to remove padding
        if decrypted.len() < frame_size {
            return Err(anyhow!("Decrypted frame too short: expected {}, got {}", frame_size, decrypted.len()));
        }
        let decrypted = &decrypted[..frame_size];
        
        let mut cursor = decrypted;
        let msg_id = u8::decode(&mut cursor).map_err(|e| anyhow!("Failed to decode msg id: {}", e))?;
        
        wasix_eth_utils::debug!("[P2P Stream] Read message ID: {}, payload size: {}", msg_id, cursor.len());

        // Determine if this message should be decompressed
        // ID >= 0x10: Subprotocol messages (eth, snap, etc.)
        // ID > 0x01 && ID < 0x10: P2P control messages (Ping, Pong, etc.)
        let is_subprotocol = msg_id >= 0x10;
        let is_compressible_p2p = msg_id > 0x01 && msg_id < 0x10;

        let payload = if (is_subprotocol || is_compressible_p2p) && self.snappy_enabled() {
            if cursor.is_empty() {
                Vec::new()
            } else {
                // 1. Verify and retrieve the decompressed length safely
                let decompressed_size = snap::raw::decompress_len(cursor).map_err(|e| {
                    wasix_eth_utils::error!("[P2P Stream] Snappy decompress_len failed for msg_id {}: {}", msg_id, e);
                    P2pError::Codec(format!("Snappy decompress_len failed: {}", e))
                })?;

                if decompressed_size > MAX_PAYLOAD_SIZE {
                    return Err(P2pError::DecompressedSizeExceedsLimit(decompressed_size, MAX_PAYLOAD_SIZE).into());
                }

                // 2. Perform decompression and propagate error if it fails
                snap::raw::Decoder::new().decompress_vec(cursor).map_err(|e| {
                    wasix_eth_utils::error!("[P2P Stream] Snappy decompression failed for msg_id {}: {}", msg_id, e);
                    P2pError::Decompression(e.to_string())
                })?
            }
        } else {
            // Raw bytes for Hello, Disconnect, or if Snappy is not enabled
            cursor.to_vec()
        };
        
        Ok((msg_id, payload))
    }
}

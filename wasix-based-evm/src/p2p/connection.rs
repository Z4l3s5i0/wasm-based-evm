use std::sync::Arc;
use crate::{error, info, debug};
use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::mpsc;
use alloy_rlp::{Encodable, Decodable, RlpEncodable, RlpDecodable};
use alloy_rlp_derive::{RlpEncodable as _, RlpDecodable as _};
use crate::p2p::swarm::PeerManager;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Ping,
    Pong,
    Hello(HelloMessage),
    PeerList(Vec<PeerInfoRlp>),
    Gossip(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
#[rlp(trailing)]
pub struct HelloMessage {
    pub peer_id: String,
    pub listen_port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
pub struct PeerInfoRlp {
    pub peer_id: String,
    pub addr: String,
}

/// A connection to a peer, encapsulating TCP.
pub struct Connection<T> {
    /// Reading side of the stream
    reader: ReadHalf<T>,
    /// Writing side of the stream
    writer: WriteHalf<T>,
    /// Channel to send messages to this peer
    send_queue: mpsc::Receiver<Message>,
}

impl<T> Connection<T>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    /// Create a new connection from an established stream.
    pub async fn new_client(
        stream: T,
        local_peer_id: String,
        listen_port: Option<u16>,
        send_queue: mpsc::Receiver<Message>,
    ) -> Result<(Self, String, Option<u16>)> {
        debug!("[P2P] Starting handshake (TLS disabled)");
        let (mut reader, mut writer) = tokio::io::split(stream);
        
        // 1. Send Hello
        Self::static_send_message(&mut writer, Message::Hello(HelloMessage { peer_id: local_peer_id, listen_port })).await?;
        
        // 2. Wait for Hello from server
        let frame = Self::static_read_frame(&mut reader).await?
            .ok_or_else(|| anyhow::anyhow!("Connection closed during handshake"))?;
        
        if frame.is_empty() || frame[0] != 2 {
            return Err(anyhow::anyhow!("Expected Hello message, got type {}", if frame.is_empty() { 255 } else { frame[0] }));
        }
        
        let hello = HelloMessage::decode(&mut &frame[1..])
            .map_err(|e| anyhow::anyhow!("RLP decode error in handshake: {}", e))?;
        let remote_peer_id = hello.peer_id;
        let remote_listen_port = hello.listen_port;
            
        Ok((Self { reader, writer, send_queue }, remote_peer_id, remote_listen_port))
    }

    pub async fn new_server(
        stream: T,
        local_peer_id: String,
        listen_port: Option<u16>,
        send_queue: mpsc::Receiver<Message>,
    ) -> Result<(Self, String, Option<u16>)> {
        debug!("[P2P] Starting handshake (TLS disabled)");
        let (mut reader, mut writer) = tokio::io::split(stream);
        
        // 1. Wait for Hello from client
        let frame = Self::static_read_frame(&mut reader).await?
            .ok_or_else(|| anyhow::anyhow!("Connection closed during handshake"))?;
        
        if frame.is_empty() || frame[0] != 2 {
            return Err(anyhow::anyhow!("Expected Hello message, got type {}", if frame.is_empty() { 255 } else { frame[0] }));
        }
        
        let hello = HelloMessage::decode(&mut &frame[1..])
            .map_err(|e| anyhow::anyhow!("RLP decode error in handshake: {}", e))?;
        let remote_peer_id = hello.peer_id;
        let remote_listen_port = hello.listen_port;
            
        // 2. Send Hello back
        Self::static_send_message(&mut writer, Message::Hello(HelloMessage { peer_id: local_peer_id, listen_port })).await?;
        
        Ok((Self { reader, writer, send_queue }, remote_peer_id, remote_listen_port))
    }

    /// Process the connection: read from network and write from the send queue.
    pub async fn process(self, peer_manager: PeerManager) -> Result<()> {
        let mut reader = self.reader;
        let mut writer = self.writer;
        let mut send_queue = self.send_queue;

        loop {
            tokio::select! {
                // Handle outgoing messages
                msg = send_queue.recv() => {
                    if let Some(msg) = msg {
                        Self::static_send_message(&mut writer, msg).await?;
                    } else {
                        break;
                    }
                }
                
                // Handle incoming data
                res = Self::static_read_frame(&mut reader) => {
                    match res {
                        Ok(Some(frame)) => {
                            Self::static_handle_frame(&mut writer, frame, &peer_manager).await?;
                        }
                        Ok(None) => {
                            info!("[P2P] Connection closed by remote");
                            break;
                        }
                        Err(e) => {
                            error!("[P2P] Read error: {}", e);
                            return Err(e);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    async fn static_send_message<W>(writer: &mut W, msg: Message) -> Result<()>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        let mut buf = Vec::new();
        match msg {
            Message::Ping => buf.push(0),
            Message::Pong => buf.push(1),
            Message::Hello(hello) => {
                buf.push(2);
                hello.encode(&mut buf);
            }
            Message::PeerList(peers) => {
                buf.push(4);
                peers.encode(&mut buf);
            }
            Message::Gossip(data) => {
                buf.push(3);
                buf.extend_from_slice(&data);
            }
        }
        let len = buf.len() as u32;
        writer.write_all(&len.to_be_bytes()).await?;
        writer.write_all(&buf).await?;
        writer.flush().await?;
        Ok(())
    }

    async fn static_read_frame<R>(reader: &mut R) -> Result<Option<Vec<u8>>>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let mut len_buf = [0u8; 4];
        let read_len = tokio::time::timeout(
            tokio::time::Duration::from_secs(60), // Wait up to 60s for a new frame
            reader.read_exact(&mut len_buf)
        ).await;

        match read_len {
            Ok(Ok(_)) => {
                let len = u32::from_be_bytes(len_buf) as usize;
                if len > 10 * 1024 * 1024 { // 10MB limit
                    return Err(anyhow::anyhow!("Frame too large: {}", len));
                }
                let mut frame = vec![0u8; len];
                // Once we have a length, we expect the payload quickly
                tokio::time::timeout(
                    tokio::time::Duration::from_secs(10),
                    reader.read_exact(&mut frame)
                ).await.map_err(|_| anyhow::anyhow!("Read payload timeout"))??;
                Ok(Some(frame))
            }
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => {
                // Read timeout
                Err(anyhow::anyhow!("Read length timeout"))
            }
        }
    }

    async fn static_handle_frame<W>(
        writer: &mut W,
        frame: Vec<u8>,
        peer_manager: &PeerManager,
    ) -> Result<()>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        if frame.is_empty() {
            return Err(anyhow::anyhow!("Empty frame"));
        }
        let msg_type = frame[0];
        let payload = &frame[1..];
        
        match msg_type {
            0 => {
                info!("[P2P] Received Ping");
                Self::static_send_message(writer, Message::Pong).await?;
            }
            1 => info!("[P2P] Received Pong"),
            2 => {
                let hello = HelloMessage::decode(&mut &payload[..])
                    .map_err(|e| anyhow::anyhow!("RLP decode error for Hello: {}", e))?;
                let peer_id = hello.peer_id;
                let listen_port = hello.listen_port;
                info!("[P2P] Received Hello from {} (listen port: {:?})", peer_id, listen_port);
            }
            3 => {
                debug!("[P2P] Received Gossip message ({} bytes)", payload.len());
            }
            4 => {
                let peers_rlp = Vec::<PeerInfoRlp>::decode(&mut &payload[..])
                    .map_err(|e| anyhow::anyhow!("RLP decode error for PeerList: {}", e))?;
                
                debug!("[P2P] Received PeerList with {} peers", peers_rlp.len());
                let pm = peer_manager.clone();
                for p_info in peers_rlp {
                    if let Ok(addr) = p_info.addr.parse::<std::net::SocketAddr>() {
                        Arc::new(pm.clone()).dial_peer(addr);
                    }
                }
            }
            _ => error!("[P2P] Unknown message type: {}", msg_type),
        }
        Ok(())
    }

    async fn send_message(&mut self, msg: Message) -> Result<()> {
        Self::static_send_message(&mut self.writer, msg).await
    }

    async fn read_frame(&mut self) -> Result<Option<Vec<u8>>> {
        Self::static_read_frame(&mut self.reader).await
    }

    async fn handle_frame(&mut self, frame: Vec<u8>, peer_manager: &crate::p2p::swarm::PeerManager) -> Result<()> {
        Self::static_handle_frame(&mut self.writer, frame, peer_manager).await
    }
}

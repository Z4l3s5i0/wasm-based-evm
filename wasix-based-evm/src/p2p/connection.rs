use crate::{error, info, debug};
use anyhow::Result;
use tokio_rustls::{TlsConnector, TlsAcceptor, TlsStream, rustls};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use alloy_rlp::{Encodable, Decodable};
use tokio_rustls::rustls::{ClientConfig, ServerConfig};

/// Message types for our custom P2P protocol
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Ping,
    Pong,
    Hello { peer_id: String },
    PeerList(Vec<(String, std::net::SocketAddr)>),
    Gossip(Vec<u8>),
}

/// A connection to a peer, encapsulating TCP and TLS.
pub struct Connection {
    /// Reading side of the TLS stream
    reader: tokio::io::ReadHalf<TlsStream<TcpStream>>,
    /// Writing side of the TLS stream
    writer: tokio::io::WriteHalf<TlsStream<TcpStream>>,
    /// Channel to send messages to this peer
    send_queue: mpsc::Receiver<Message>,
}

impl Connection {
    /// Create a new connection from an established TCP stream.
    /// In a real P2P system, we'd perform a TLS handshake here.
    pub async fn new_client(
        stream: TcpStream,
        config: Arc<ClientConfig>,
        server_name: rustls::ServerName,
        local_peer_id: String,
        send_queue: mpsc::Receiver<Message>,
    ) -> Result<(Self, String)> {
        let connector = TlsConnector::from(config);
        let tls_stream = connector.connect(server_name, stream).await?;
        let (mut reader, mut writer) = tokio::io::split(TlsStream::Client(tls_stream));
        
        // 1. Send Hello
        Self::static_send_message(&mut writer, Message::Hello { peer_id: local_peer_id }).await?;
        
        // 2. Wait for Hello from server
        let frame = Self::static_read_frame(&mut reader).await?
            .ok_or_else(|| anyhow::anyhow!("Connection closed during handshake"))?;
        
        if frame.is_empty() || frame[0] != 2 {
            return Err(anyhow::anyhow!("Expected Hello message, got type {}", if frame.is_empty() { 255 } else { frame[0] }));
        }
        
        let remote_peer_id = String::decode(&mut &frame[1..])
            .map_err(|e| anyhow::anyhow!("RLP decode error in handshake: {}", e))?;
            
        Ok((Self { reader, writer, send_queue }, remote_peer_id))
    }

    pub async fn new_server(
        stream: TcpStream,
        config: Arc<ServerConfig>,
        local_peer_id: String,
        send_queue: mpsc::Receiver<Message>,
    ) -> Result<(Self, String)> {
        let acceptor = TlsAcceptor::from(config);
        let tls_stream = acceptor.accept(stream).await?;
        let (mut reader, mut writer) = tokio::io::split(TlsStream::Server(tls_stream));
        
        // 1. Wait for Hello from client
        let frame = Self::static_read_frame(&mut reader).await?
            .ok_or_else(|| anyhow::anyhow!("Connection closed during handshake"))?;
        
        if frame.is_empty() || frame[0] != 2 {
            return Err(anyhow::anyhow!("Expected Hello message, got type {}", if frame.is_empty() { 255 } else { frame[0] }));
        }
        
        let remote_peer_id = String::decode(&mut &frame[1..])
            .map_err(|e| anyhow::anyhow!("RLP decode error in handshake: {}", e))?;
            
        // 2. Send Hello back
        Self::static_send_message(&mut writer, Message::Hello { peer_id: local_peer_id }).await?;
        
        Ok((Self { reader, writer, send_queue }, remote_peer_id))
    }

    /// Process the connection: read from network and write from the send queue.
    pub async fn process(self) -> Result<()> {
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
                            Self::static_handle_frame(&mut writer, frame).await?;
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

    async fn static_send_message(writer: &mut tokio::io::WriteHalf<TlsStream<TcpStream>>, msg: Message) -> Result<()> {
        let mut buf = Vec::new();
        match msg {
            Message::Ping => buf.push(0),
            Message::Pong => buf.push(1),
            Message::Hello { peer_id } => {
                buf.push(2);
                peer_id.encode(&mut buf);
            }
            Message::PeerList(peers) => {
                buf.push(4);
                // RLP doesn't naturally support tuples as Encodable.
                // We'll encode each pair as a Vec of two strings.
                let encoded_peers: Vec<Vec<String>> = peers.into_iter()
                    .map(|(id, addr)| vec![id, addr.to_string()])
                    .collect();
                encoded_peers.encode(&mut buf);
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

    async fn static_read_frame(reader: &mut tokio::io::ReadHalf<TlsStream<TcpStream>>) -> Result<Option<Vec<u8>>> {
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf).await {
            Ok(_) => {
                let len = u32::from_be_bytes(len_buf) as usize;
                if len > 10 * 1024 * 1024 { // 10MB limit
                    return Err(anyhow::anyhow!("Frame too large: {}", len));
                }
                let mut frame = vec![0u8; len];
                reader.read_exact(&mut frame).await?;
                Ok(Some(frame))
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn static_handle_frame(writer: &mut tokio::io::WriteHalf<TlsStream<TcpStream>>, frame: Vec<u8>) -> Result<()> {
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
                let peer_id = String::decode(&mut &payload[..])
                    .map_err(|e| anyhow::anyhow!("RLP decode error: {}", e))?;
                info!("[P2P] Received Hello from {}", peer_id);
            }
            3 => {
                debug!("[P2P] Received Gossip message ({} bytes)", payload.len());
            }
            4 => {
                let encoded_peers = Vec::<Vec<String>>::decode(&mut &payload[..])
                    .map_err(|e| anyhow::anyhow!("RLP decode error for PeerList: {}", e))?;
                let mut peers = Vec::new();
                for pair in encoded_peers {
                    if pair.len() == 2 {
                        let id = pair[0].clone();
                        if let Ok(addr) = pair[1].parse::<std::net::SocketAddr>() {
                            peers.push((id, addr));
                        }
                    }
                }
                debug!("[P2P] Received PeerList with {} peers", peers.len());
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

    async fn handle_frame(&mut self, frame: Vec<u8>) -> Result<()> {
        Self::static_handle_frame(&mut self.writer, frame).await
    }
}

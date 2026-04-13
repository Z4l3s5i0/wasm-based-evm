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
    // Add more message types as needed
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
        send_queue: mpsc::Receiver<Message>,
    ) -> Result<Self> {
        let connector = TlsConnector::from(config);
        let tls_stream = connector.connect(server_name, stream).await?;
        let (reader, writer) = tokio::io::split(TlsStream::Client(tls_stream));
        Ok(Self { reader, writer, send_queue })
    }

    pub async fn new_server(
        stream: TcpStream,
        config: Arc<ServerConfig>,
        send_queue: mpsc::Receiver<Message>,
    ) -> Result<Self> {
        let acceptor = TlsAcceptor::from(config);
        let tls_stream = acceptor.accept(stream).await?;
        let (reader, writer) = tokio::io::split(TlsStream::Server(tls_stream));
        Ok(Self { reader, writer, send_queue })
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

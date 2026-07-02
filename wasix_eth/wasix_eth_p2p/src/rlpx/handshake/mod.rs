pub mod ecies;
pub mod protocol;

use std::sync::Arc;
use tokio::net::TcpStream;
use crate::rlpx::RlpxStream;
use crate::rlpx::message::StatusMessage;
use crate::peer::peer_registry::PeerRegistry;
use wasix_eth_utils::{info};

pub struct Handshake {
    registry: Arc<PeerRegistry>,
}

impl Handshake {
    pub fn new(registry: Arc<PeerRegistry>) -> Self {
        Self { registry }
    }

    pub async fn handle_inbound(&self, stream: TcpStream) -> anyhow::Result<(RlpxStream<TcpStream>, StatusMessage)> {
        info!("[P2P Handshake] Inbound: accepting RLPx stream");
        let mut rlpx_stream = RlpxStream::accept(stream).await?;
        tokio::task::yield_now().await;
        let local_sk = self.registry.local_identity().secret_key();
        
        info!("[P2P Handshake] Inbound: starting ECIES handshake");
        ecies::do_handshake(&mut rlpx_stream, false, &local_sk, None).await?;
        info!("[P2P Handshake] Inbound: ECIES handshake completed");
        tokio::task::yield_now().await;

        info!("[P2P Handshake] Inbound: starting P2P Hello handshake");
        let local_hello = protocol::create_local_hello(&self.registry);
        protocol::do_p2p_handshake(&mut rlpx_stream, local_hello).await?;
        info!("[P2P Handshake] Inbound: P2P Hello handshake completed");
        tokio::task::yield_now().await;

        info!("[P2P Handshake] Inbound: starting ETH Status handshake");
        let local_status = protocol::create_local_status(&self.registry, &rlpx_stream).await?;
        let remote_status = protocol::do_eth_handshake(&self.registry, &mut rlpx_stream, local_status).await?;
        info!("[P2P Handshake] Inbound: ETH Status handshake completed");

        Ok((rlpx_stream, remote_status))
    }

    pub async fn handle_outbound(&self, mut rlpx_stream: RlpxStream<TcpStream>, remote_pk: &k256::PublicKey) -> anyhow::Result<(RlpxStream<TcpStream>, StatusMessage)> {
        let local_sk = self.registry.local_identity().secret_key();

        info!("[P2P Handshake] Outbound: starting ECIES handshake");
        ecies::do_handshake(&mut rlpx_stream, true, &local_sk, Some(remote_pk)).await?;
        info!("[P2P Handshake] Outbound: ECIES handshake completed");
        tokio::task::yield_now().await;

        info!("[P2P Handshake] Outbound: starting P2P Hello handshake");
        let local_hello = protocol::create_local_hello(&self.registry);
        protocol::do_p2p_handshake(&mut rlpx_stream, local_hello).await?;
        info!("[P2P Handshake] Outbound: P2P Hello handshake completed");
        tokio::task::yield_now().await;

        info!("[P2P Handshake] Outbound: starting ETH Status handshake");
        let local_status = protocol::create_local_status(&self.registry, &rlpx_stream).await?;
        let remote_status = protocol::do_eth_handshake(&self.registry, &mut rlpx_stream, local_status).await?;
        info!("[P2P Handshake] Outbound: ETH Status handshake completed");

        Ok((rlpx_stream, remote_status))
    }
}

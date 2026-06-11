pub mod ecies;
pub mod protocol;

use std::sync::Arc;
use tokio::net::TcpStream;
use crate::rlpx::RlpxStream;
use crate::rlpx::message::StatusMessage;
use crate::peer::peer_registry::PeerRegistry;

pub struct Handshake {
    registry: Arc<PeerRegistry>,
}

impl Handshake {
    pub fn new(registry: Arc<PeerRegistry>) -> Self {
        Self { registry }
    }

    pub async fn handle_inbound(&self, stream: TcpStream) -> anyhow::Result<(RlpxStream<TcpStream>, StatusMessage)> {
        let mut rlpx_stream = RlpxStream::accept(stream).await?;
        let local_sk = self.registry.local_identity().secret_key();
        
        // 1. ECIES Handshake
        ecies::do_handshake(&mut rlpx_stream, false, &local_sk, None).await?;

        // 2. P2P Handshake (Hello)
        let local_hello = protocol::create_local_hello(&self.registry);
        protocol::do_p2p_handshake(&mut rlpx_stream, local_hello).await?;

        // 3. ETH Handshake (Status)
        let local_status = protocol::create_local_status(&self.registry, &rlpx_stream).await?;
        let remote_status = protocol::do_eth_handshake(&self.registry, &mut rlpx_stream, local_status).await?;

        Ok((rlpx_stream, remote_status))
    }

    pub async fn handle_outbound(&self, mut rlpx_stream: RlpxStream<TcpStream>, remote_pk: &k256::PublicKey) -> anyhow::Result<(RlpxStream<TcpStream>, StatusMessage)> {
        let local_sk = self.registry.local_identity().secret_key();

        // 1. ECIES Handshake
        ecies::do_handshake(&mut rlpx_stream, true, &local_sk, Some(remote_pk)).await?;

        // 2. P2P Handshake (Hello)
        let local_hello = protocol::create_local_hello(&self.registry);
        protocol::do_p2p_handshake(&mut rlpx_stream, local_hello).await?;

        // 3. ETH Handshake (Status)
        let local_status = protocol::create_local_status(&self.registry, &rlpx_stream).await?;
        let remote_status = protocol::do_eth_handshake(&self.registry, &mut rlpx_stream, local_status).await?;

        Ok((rlpx_stream, remote_status))
    }
}

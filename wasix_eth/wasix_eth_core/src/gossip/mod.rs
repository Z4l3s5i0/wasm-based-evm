pub mod gossip_handler;
pub mod gossip_bridge;

pub use wasix_eth_types::{GossipProvider, NoopGossip};
pub use gossip_handler::GossipService;
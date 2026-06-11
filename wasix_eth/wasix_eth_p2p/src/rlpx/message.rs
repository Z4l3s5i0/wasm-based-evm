pub use wasix_eth_types::p2p::{
    Hello, Capability, Disconnect, Ping, Pong, Status, ForkId, 
    GetBlockHeaders, BlockHeaders, GetBlockBodies, BlockBodies, BlockHashOrNumber,
    StatusMessage, StatusEth69, EthVersion, EthMessageID, RequestPair,
    NewPooledTransactionHashes66, NewPooledTransactionHashes, BlockRangeUpdate,
    SnapMessageID, GetAccountRange, AccountRange, GetStorageRanges, StorageRanges,
    GetByteCodes, ByteCodes, GetTrieNodes, TrieNodes
};

pub enum P2PMessage {
    Hello(Hello),
    Disconnect(Disconnect),
    Ping,
    Pong,
}

impl P2PMessage {
    pub fn id(&self) -> u8 {
        match self {
            P2PMessage::Hello(_) => 0x00,
            P2PMessage::Disconnect(_) => 0x01,
            P2PMessage::Ping => 0x02,
            P2PMessage::Pong => 0x03,
        }
    }
}

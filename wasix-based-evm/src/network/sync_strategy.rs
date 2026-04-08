use crate::network::protocol::{GetBlockHeaders, GetBlockBodies, BlockHashOrNumber};
use crate::storage::Block;
use tentacle::SessionId;
use std::collections::VecDeque;

pub enum SyncAction {
    RequestHeaders(SessionId, GetBlockHeaders),
    RequestBodies(SessionId, GetBlockBodies),
    None,
}

pub struct SyncStrategy {
    pub pending_headers: VecDeque<Block>,
    pub next_request_id: u64,
}

impl SyncStrategy {
    pub fn new() -> Self {
        Self {
            pending_headers: VecDeque::new(),
            next_request_id: 1,
        }
    }

    pub fn next_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        id
    }

    pub fn request_headers(&mut self, session_id: SessionId, latest_number: u64) -> SyncAction {
        let request_id = self.next_request_id();
        let request = GetBlockHeaders {
            request_id,
            block: BlockHashOrNumber::Number(latest_number + 1),
            amount: 10,
            skip: 0,
            reverse: false,
        };
        SyncAction::RequestHeaders(session_id, request)
    }

    pub fn handle_headers(&mut self, session_id: SessionId, headers: Vec<Block>) -> SyncAction {
        if headers.is_empty() {
            return SyncAction::None;
        }

        let mut hashes = Vec::new();
        for header in headers {
            hashes.push(header.body.execution_payload.block_hash.clone());
            self.pending_headers.push_back(header);
        }

        let request_id = self.next_request_id();
        let request = GetBlockBodies {
            request_id,
            hashes,
        };

        SyncAction::RequestBodies(session_id, request)
    }
}

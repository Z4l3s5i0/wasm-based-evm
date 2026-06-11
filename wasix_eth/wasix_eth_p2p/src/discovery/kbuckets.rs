use alloy_primitives::{B512, keccak256, B256};
use crate::discovery::v4::NodeEndpoint;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct NodeRecord {
    pub id: B512,
    pub endpoint: NodeEndpoint,
    pub last_seen: Instant,
}

pub struct RoutingTable {
    local_id: B512,
    local_id_hash: B256,
    buckets: Vec<Vec<NodeRecord>>,
    max_bucket_size: usize,
}

impl RoutingTable {
    pub fn new(local_id: B512) -> Self {
        let local_id_hash = keccak256(local_id.as_slice());
        Self {
            local_id,
            local_id_hash,
            buckets: vec![vec![]; 256], // Using hash-based distance (256 bits)
            max_bucket_size: 16,
        }
    }

    fn distance_log2(&self, id: B512) -> Option<usize> {
        let id_hash = keccak256(id.as_slice());
        let xor = self.local_id_hash ^ id_hash;
        
        for (i, byte) in xor.iter().enumerate() {
            if *byte != 0 {
                return Some(255 - (i * 8 + byte.leading_zeros() as usize));
            }
        }
        None // Same ID
    }

    pub fn add_node(&mut self, id: B512, endpoint: NodeEndpoint) -> Option<NodeRecord> {
        if id == self.local_id {
            return None;
        }

        if let Some(bucket_idx) = self.distance_log2(id) {
            let bucket = &mut self.buckets[bucket_idx];
            
            if let Some(existing) = bucket.iter_mut().find(|n| n.id == id) {
                existing.endpoint = endpoint;
                existing.last_seen = Instant::now();
                // Move to end (most recently seen)
                let record = existing.clone();
                bucket.retain(|n| n.id != id);
                bucket.push(record);
                return None;
            } else if bucket.len() < self.max_bucket_size {
                bucket.push(NodeRecord {
                    id,
                    endpoint,
                    last_seen: Instant::now(),
                });
                return None;
            } else {
                // Bucket is full. Return oldest node to be pinged.
                return Some(bucket[0].clone());
            }
        }
        None
    }

    pub fn remove_node(&mut self, id: B512) {
        if let Some(bucket_idx) = self.distance_log2(id) {
            self.buckets[bucket_idx].retain(|n| n.id != id);
        }
    }

    pub fn closest_nodes(&self, target: B512, count: usize) -> Vec<NodeRecord> {
        let target_hash = keccak256(target.as_slice());
        let mut all_nodes: Vec<NodeRecord> = self.buckets.iter().flatten().cloned().collect();
        
        all_nodes.sort_by_key(|n| {
            let n_hash = keccak256(n.id.as_slice());
            target_hash ^ n_hash
        });

        all_nodes.into_iter().take(count).collect()
    }

    pub fn get_all_nodes(&self) -> Vec<NodeRecord> {
        self.buckets.iter().flatten().cloned().collect()
    }
}

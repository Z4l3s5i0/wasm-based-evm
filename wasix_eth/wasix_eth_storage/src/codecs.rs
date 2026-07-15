use wasix_eth_types::{Header, B256, Address, U256, Receipt, ReceiptMeta, Transaction, Bytes, BlockBody, TrieAccount, PayloadId, Block, PeerEntry, BlobsBundleV1};
use crate::tables::*;
use std::fmt::Debug;
use alloy_rlp::{Decodable, Encodable, Buf};

use redb::{Value, TableDefinition, Key};

pub trait Table: Debug {
    const NAME: &'static str;
    type Key: RedbRlp + 'static;
    type Value: RedbRlp + 'static;

    fn definition() -> TableDefinition<'static, RlpValue<Self::Key>, RlpValue<Self::Value>> {
        TableDefinition::new(Self::NAME)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RlpValue<T>(pub T);

pub trait RedbRlp: Debug + 'static {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut);
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> where Self: Sized;
    fn rlp_length(&self) -> usize;
}

macro_rules! impl_redb_rlp {
    ($t:ty) => {
        impl RedbRlp for $t {
            fn encode(&self, out: &mut dyn alloy_rlp::BufMut) { Encodable::encode(self, out); }
            fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> { Decodable::decode(buf) }
            fn rlp_length(&self) -> usize { Encodable::length(self) }
        }
    };
}

impl RedbRlp for u64 {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) { out.put_u64(*self); }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        if buf.is_empty() {
            return Err(alloy_rlp::Error::InputTooShort);
        }
        if buf.len() == 8 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&buf[..8]);
            *buf = &buf[8..];
            Ok(u64::from_be_bytes(b))
        } else {
            // Fallback to RLP for backward compatibility with existing data
            Decodable::decode(buf)
        }
    }
    fn rlp_length(&self) -> usize { 8 }
}
impl_redb_rlp!(Header);
impl_redb_rlp!(B256);
impl_redb_rlp!(Address);
impl_redb_rlp!(U256);
impl_redb_rlp!(Transaction);
impl_redb_rlp!(Receipt);
impl_redb_rlp!(BlockBody<Transaction>);
impl_redb_rlp!(TrieAccount);
impl_redb_rlp!(Bytes);

impl RedbRlp for String {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) { Encodable::encode(self, out); }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        Decodable::decode(buf)
    }
    fn rlp_length(&self) -> usize { Encodable::length(self) }
}

impl RedbRlp for PayloadId {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) { Encodable::encode(&self.0, out); }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> { Ok(PayloadId::new(Decodable::decode(buf)?)) }
    fn rlp_length(&self) -> usize { Encodable::length(&self.0) }
}

impl RedbRlp for (Address, B256) {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let payload_length = Encodable::length(&self.0) + Encodable::length(&self.1);
        alloy_rlp::Header { list: true, payload_length }.encode(out);
        Encodable::encode(&self.0, out);
        Encodable::encode(&self.1, out);
    }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        let _h = alloy_rlp::Header::decode(buf)?;
        Ok((Decodable::decode(buf)?, Decodable::decode(buf)?))
    }
    fn rlp_length(&self) -> usize {
        let l = Encodable::length(&self.0) + Encodable::length(&self.1);
        alloy_rlp::length_of_length(l) + l
    }
}

impl RedbRlp for Vec<(Address, Option<Bytes>)> {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let mut payload_length = 0;
        for (a, b) in self {
            let item_len = Encodable::length(a) + match b {
                Some(v) => Encodable::length(v),
                None => 1,
            };
            payload_length += alloy_rlp::length_of_length(item_len) + item_len;
        }
        alloy_rlp::Header { list: true, payload_length }.encode(out);
        for (a, b) in self {
            let item_len = Encodable::length(a) + match b {
                Some(v) => Encodable::length(v),
                None => 1,
            };
            alloy_rlp::Header { list: true, payload_length: item_len }.encode(out);
            Encodable::encode(a, out);
            match b {
                Some(v) => Encodable::encode(v, out),
                None => out.put_u8(0xfb), // Use a non-RLP byte for None
            }
        }
    }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        let h = alloy_rlp::Header::decode(buf)?;
        let mut res = Vec::new();
        let mut remaining = &buf[..h.payload_length];
        *buf = &buf[h.payload_length..];
        while !remaining.is_empty() {
            let _item_h = alloy_rlp::Header::decode(&mut remaining)?;
            let a = Decodable::decode(&mut remaining)?;
            let b = if remaining[0] == 0xfb {
                remaining = &remaining[1..];
                None
            } else {
                Some(Decodable::decode(&mut remaining)?)
            };
            res.push((a, b));
        }
        Ok(res)
    }
    fn rlp_length(&self) -> usize {
        let mut payload_length = 0;
        for (a, b) in self {
            let item_len = Encodable::length(a) + match b {
                Some(v) => Encodable::length(v),
                None => 1,
            };
            payload_length += alloy_rlp::length_of_length(item_len) + item_len;
        }
        alloy_rlp::length_of_length(payload_length) + payload_length
    }
}

impl RedbRlp for Vec<(Address, B256, U256)> {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let mut payload_length = 0;
        for (a, b, c) in self {
            let item_len = Encodable::length(a) + Encodable::length(b) + Encodable::length(c);
            payload_length += alloy_rlp::length_of_length(item_len) + item_len;
        }
        alloy_rlp::Header { list: true, payload_length }.encode(out);
        for (a, b, c) in self {
            let item_len = Encodable::length(a) + Encodable::length(b) + Encodable::length(c);
            alloy_rlp::Header { list: true, payload_length: item_len }.encode(out);
            Encodable::encode(a, out);
            Encodable::encode(b, out);
            Encodable::encode(c, out);
        }
    }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        let h = alloy_rlp::Header::decode(buf)?;
        let mut res = Vec::new();
        let mut remaining = &buf[..h.payload_length];
        *buf = &buf[h.payload_length..];
        while !remaining.is_empty() {
            let _item_h = alloy_rlp::Header::decode(&mut remaining)?;
            res.push((Decodable::decode(&mut remaining)?, Decodable::decode(&mut remaining)?, Decodable::decode(&mut remaining)?));
        }
        Ok(res)
    }
    fn rlp_length(&self) -> usize {
        let mut payload_length = 0;
        for (a, b, c) in self {
            let item_len = Encodable::length(a) + Encodable::length(b) + Encodable::length(c);
            payload_length += alloy_rlp::length_of_length(item_len) + item_len;
        }
        alloy_rlp::length_of_length(payload_length) + payload_length
    }
}

impl RedbRlp for (Block<Transaction>, Vec<Receipt>, BlobsBundleV1) {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let item_len_0 = Encodable::length(&self.0);
        let item_len_1 = Encodable::length(&self.1);
        
        // BlobsBundleV1 doesn't implement Encodable, so we encode its fields manually as a sublist
        let commitments_len = Encodable::length(&self.2.commitments);
        let proofs_len = Encodable::length(&self.2.proofs);
        let blobs_len = Encodable::length(&self.2.blobs);
        let bundle_payload_len = commitments_len + proofs_len + blobs_len;
        let bundle_total_len = alloy_rlp::length_of_length(bundle_payload_len) + bundle_payload_len;

        let payload_length = item_len_0 + item_len_1 + bundle_total_len;
        alloy_rlp::Header { list: true, payload_length }.encode(out);
        
        Encodable::encode(&self.0, out);
        Encodable::encode(&self.1, out);
        
        // Encode bundle as sublist
        alloy_rlp::Header { list: true, payload_length: bundle_payload_len }.encode(out);
        Encodable::encode(&self.2.commitments, out);
        Encodable::encode(&self.2.proofs, out);
        Encodable::encode(&self.2.blobs, out);
    }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        let _h = alloy_rlp::Header::decode(buf)?;
        let block = Decodable::decode(buf)?;
        let receipts = Decodable::decode(buf)?;
        
        let _h_bundle = alloy_rlp::Header::decode(buf)?;
        let bundle = BlobsBundleV1 {
            commitments: Decodable::decode(buf)?,
            proofs: Decodable::decode(buf)?,
            blobs: Decodable::decode(buf)?,
        };
        
        Ok((block, receipts, bundle))
    }
    fn rlp_length(&self) -> usize {
        let item_len_0 = Encodable::length(&self.0);
        let item_len_1 = Encodable::length(&self.1);
        
        let bundle_payload_len = Encodable::length(&self.2.commitments) + 
                                Encodable::length(&self.2.proofs) + 
                                Encodable::length(&self.2.blobs);
        let bundle_total_len = alloy_rlp::length_of_length(bundle_payload_len) + bundle_payload_len;

        let l = item_len_0 + item_len_1 + bundle_total_len;
        alloy_rlp::length_of_length(l) + l
    }
}

impl RedbRlp for (B256, u64) {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let payload_length = Encodable::length(&self.0) + Encodable::length(&self.1);
        alloy_rlp::Header { list: true, payload_length }.encode(out);
        Encodable::encode(&self.0, out);
        Encodable::encode(&self.1, out);
    }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        let _h = alloy_rlp::Header::decode(buf)?;
        Ok((Decodable::decode(buf)?, Decodable::decode(buf)?))
    }
    fn rlp_length(&self) -> usize {
        let l = Encodable::length(&self.0) + Encodable::length(&self.1);
        alloy_rlp::length_of_length(l) + l
    }
}

impl<T: RedbRlp> Value for RlpValue<T> {
    type SelfType<'a> = T;
    type AsBytes<'a> = Vec<u8>;
    fn fixed_width() -> Option<usize> { None }
    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a> where Self: 'a {
        let mut d = data;
        match T::decode(&mut d) {
            Ok(v) => v,
            Err(e) => panic!("Failed to decode {}: {:?}", std::any::type_name::<T>(), e),
        }
    }
    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a> where Self: 'a, Self: 'b {
        let mut buf = Vec::with_capacity(value.rlp_length());
        value.encode(&mut buf);
        buf
    }
    fn type_name() -> redb::TypeName { redb::TypeName::new(std::any::type_name::<T>()) }
}

impl<T: RedbRlp> Key for RlpValue<T> {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering { data1.cmp(data2) }
}
impl RedbRlp for PeerEntry {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let payload_length = Encodable::length(&self.peer_id) + Encodable::length(&self.discovery_addr.to_string()) + Encodable::length(&self.p2p_addr.to_string());
        alloy_rlp::Header { list: true, payload_length }.encode(out);
        Encodable::encode(&self.peer_id, out);
        Encodable::encode(&self.discovery_addr.to_string(), out);
        Encodable::encode(&self.p2p_addr.to_string(), out);
    }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        let h = alloy_rlp::Header::decode(buf)?;
        if !h.list {
             return Err(alloy_rlp::Error::Custom("Expected list for PeerEntry"));
        }
        let peer_id: String = Decodable::decode(buf)?;
        let disc_addr_str: String = Decodable::decode(buf)?;
        let p2p_addr_str: String = Decodable::decode(buf)?;
        
        Ok(PeerEntry {
            peer_id,
            discovery_addr: disc_addr_str.parse().map_err(|_| alloy_rlp::Error::Custom("Invalid discovery_addr"))?,
            p2p_addr: p2p_addr_str.parse().map_err(|_| alloy_rlp::Error::Custom("Invalid p2p_addr"))?,
        })
    }
    fn rlp_length(&self) -> usize {
        let l = Encodable::length(&self.peer_id) + Encodable::length(&self.discovery_addr.to_string()) + Encodable::length(&self.p2p_addr.to_string());
        alloy_rlp::length_of_length(l) + l
    }
}

impl RedbRlp for ReceiptMeta {
    fn encode(&self, out: &mut dyn alloy_rlp::BufMut) {
        let payload_length = match self.contract_address {
            Some(_) => 21, // 1 byte header + 20 bytes address
            None => 1,    // 1 byte header
        };
        alloy_rlp::Header { list: true, payload_length }.encode(out);
        match self.contract_address {
            Some(addr) => Encodable::encode(&addr, out),
            None => out.put_u8(0x80), // RLP empty string
        }
    }
    fn decode(buf: &mut &[u8]) -> Result<Self, alloy_rlp::Error> {
        let h = alloy_rlp::Header::decode(buf)?;
        if !h.list {
            return Err(alloy_rlp::Error::Custom("Expected list for ReceiptMeta"));
        }
        
        let contract_address = if buf.is_empty() {
             None
        } else {
             let item_h = alloy_rlp::Header::decode(&mut &**buf)?;
             if item_h.payload_length == 0 {
                 let _ = buf.get_u8(); // consume empty string
                 None
             } else {
                 Some(<Address as Decodable>::decode(buf)?)
             }
        };
        
        Ok(ReceiptMeta { contract_address })
    }
    fn rlp_length(&self) -> usize {
        let l = match self.contract_address {
            Some(_) => 21,
            None => 1,
        };
        alloy_rlp::length_of_length(l) + l
    }
}

impl Table for Headers { const NAME: &'static str = "headers"; type Key = B256; type Value = Header; }
impl Table for HeaderTD { const NAME: &'static str = "header_td"; type Key = B256; type Value = U256; }
impl Table for BlockBodies { const NAME: &'static str = "block_bodies"; type Key = B256; type Value = BlockBody<Transaction>; }
impl Table for Transactions { const NAME: &'static str = "transactions"; type Key = B256; type Value = Transaction; }
impl Table for Receipts { const NAME: &'static str = "receipts"; type Key = (B256, u64); type Value = Receipt; }
impl Table for ReceiptsMeta { const NAME: &'static str = "receipts_meta"; type Key = (B256, u64); type Value = ReceiptMeta; }
impl Table for CanonicalHeads { const NAME: &'static str = "canonical_heads"; type Key = u64; type Value = B256; }
impl Table for HeaderNumbers { const NAME: &'static str = "header_numbers"; type Key = B256; type Value = u64; }
impl Table for TransactionLookup { const NAME: &'static str = "transaction_lookup"; type Key = B256; type Value = (B256, u64); }
impl Table for Accounts { const NAME: &'static str = "accounts"; type Key = Address; type Value = TrieAccount; }
impl Table for Storages { const NAME: &'static str = "storages"; type Key = (Address, B256); type Value = U256; }
impl Table for Bytecodes { const NAME: &'static str = "bytecodes"; type Key = B256; type Value = Bytes; }
impl Table for AccountChangeSets { const NAME: &'static str = "account_change_sets"; type Key = u64; type Value = Vec<(Address, Option<Bytes>)>; }
impl Table for StorageChangeSets { const NAME: &'static str = "storage_change_sets"; type Key = u64; type Value = Vec<(Address, B256, U256)>; }
impl Table for PlainState { const NAME: &'static str = "plain_state"; type Key = Address; type Value = Bytes; }
impl Table for HashedState { const NAME: &'static str = "hashed_state"; type Key = B256; type Value = Bytes; }
impl Table for TrieNodes { const NAME: &'static str = "trie_nodes"; type Key = B256; type Value = Bytes; }
impl Table for Metadata { const NAME: &'static str = "metadata"; type Key = String; type Value = Bytes; }
impl Table for Payloads { const NAME: &'static str = "payloads"; type Key = PayloadId; type Value = (Block<Transaction>, Vec<Receipt>, BlobsBundleV1); }

impl Table for Forkchoice { const NAME: &'static str = "forkchoice"; type Key = String; type Value = B256; }
impl Table for ActivePeers { const NAME: &'static str = "active_peers"; type Key = String; type Value = PeerEntry; }

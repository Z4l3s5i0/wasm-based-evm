use wasix_eth_storage::codecs::{RedbRlp, RlpValue};
use wasix_eth_types::*;
use redb::{Value};

// --- u64 codec ---

#[test]
fn test_codec_u64_success() {
    let val: u64 = 12345678;
    let mut buf = Vec::new();
    RedbRlp::encode(&val, &mut buf);
    assert_eq!(buf.len(), 8);
    let mut slice = &buf[..];
    let decoded: u64 = RedbRlp::decode(&mut slice).unwrap();
    assert_eq!(decoded, val);
}

#[test]
fn test_codec_u64_failure_short_buffer() {
    let mut slice: &[u8] = &[];
    let res: Result<u64, _> = RedbRlp::decode(&mut slice);
    assert!(res.is_err());
}

#[test]
fn test_codec_u64_edge_zero() {
    let val: u64 = 0;
    let mut buf = Vec::new();
    RedbRlp::encode(&val, &mut buf);
    let mut slice = &buf[..];
    let decoded: u64 = RedbRlp::decode(&mut slice).unwrap();
    assert_eq!(decoded, val);
}

// --- Header codec ---

#[test]
fn test_codec_header_success() {
    let header = Header { number: 100, ..Default::default() };
    let bytes = RlpValue::<Header>::as_bytes(&header);
    let decoded = RlpValue::<Header>::from_bytes(&bytes);
    assert_eq!(decoded.number, 100);
}

#[test]
fn test_codec_header_failure_corrupt() {
    let bytes = vec![0xff, 0xff];
    let res = std::panic::catch_unwind(|| {
        RlpValue::<Header>::from_bytes(&bytes)
    });
    assert!(res.is_err());
}

#[test]
fn test_codec_header_edge_empty_fields() {
    let header = Header::default();
    let bytes = RlpValue::<Header>::as_bytes(&header);
    let decoded = RlpValue::<Header>::from_bytes(&bytes);
    assert_eq!(decoded.number, 0);
}

// --- String codec ---

#[test]
fn test_codec_string_success() {
    let s = "hello".to_string();
    let mut buf = Vec::new();
    RedbRlp::encode(&s, &mut buf);
    let mut slice = &buf[..];
    let decoded: String = RedbRlp::decode(&mut slice).unwrap();
    assert_eq!(decoded, s);
}

#[test]
fn test_codec_string_failure_invalid_rlp() {
    let mut slice: &[u8] = &[0x80 + 10, 1, 2]; // Claims 10 bytes, only has 2
    let res: Result<String, _> = RedbRlp::decode(&mut slice);
    assert!(res.is_err());
}

#[test]
fn test_codec_string_edge_empty() {
    let s = "".to_string();
    let mut buf = Vec::new();
    RedbRlp::encode(&s, &mut buf);
    let mut slice = &buf[..];
    let decoded: String = RedbRlp::decode(&mut slice).unwrap();
    assert_eq!(decoded, s);
}

// --- Vec<(Address, Option<Bytes>)> codec ---

#[test]
fn test_codec_change_set_success() {
    let changes = vec![(Address::from([0x12; 20]), Some(vec![1, 2, 3].into()))];
    let mut buf = Vec::new();
    RedbRlp::encode(&changes, &mut buf);
    let mut slice = &buf[..];
    let decoded: Vec<(Address, Option<Bytes>)> = RedbRlp::decode(&mut slice).unwrap();
    assert_eq!(decoded, changes);
}

#[test]
fn test_codec_change_set_failure_truncated() {
    let changes = vec![(Address::from([0x12; 20]), Some(vec![1, 2, 3].into()))];
    let mut buf = Vec::new();
    RedbRlp::encode(&changes, &mut buf);
    let mut slice = &buf[..buf.len()-1];
    let res: Result<Vec<(Address, Option<Bytes>)>, _> = RedbRlp::decode(&mut slice);
    assert!(res.is_err());
}

#[test]
fn test_codec_change_set_edge_none() {
    let changes = vec![(Address::from([0x12; 20]), None)];
    let mut buf = Vec::new();
    RedbRlp::encode(&changes, &mut buf);
    let mut slice = &buf[..];
    let decoded: Vec<(Address, Option<Bytes>)> = RedbRlp::decode(&mut slice).unwrap();
    assert_eq!(decoded, changes);
}

// --- PeerEntry codec ---

#[test]
fn test_codec_peer_entry_success() {
    let peer = PeerEntry {
        peer_id: "test".into(),
        discovery_addr: "127.0.0.1:1234".parse().unwrap(),
        p2p_addr: "127.0.0.1:5678".parse().unwrap(),
    };
    let mut buf = Vec::new();
    RedbRlp::encode(&peer, &mut buf);
    let mut slice = &buf[..];
    let decoded: PeerEntry = RedbRlp::decode(&mut slice).unwrap();
    assert_eq!(decoded.peer_id, peer.peer_id);
}

#[test]
fn test_codec_peer_entry_failure_invalid_addr() {
    // Manually construct invalid RLP for PeerEntry
    // PeerEntry is (String, String, String)
    let mut buf = Vec::new();
    let mut items = Vec::new();
    alloy_rlp::Encodable::encode(&"id", &mut items);
    alloy_rlp::Encodable::encode(&"not_an_addr", &mut items);
    alloy_rlp::Encodable::encode(&"127.0.0.1:2", &mut items);
    
    alloy_rlp::Header { list: true, payload_length: items.len() }.encode(&mut buf);
    buf.extend(items);
    
    let mut slice = &buf[..];
    let res: Result<PeerEntry, _> = RedbRlp::decode(&mut slice);
    assert!(res.is_err());
}

#[test]
fn test_codec_peer_entry_edge_long_id() {
    let peer = PeerEntry {
        peer_id: "a".repeat(1000),
        discovery_addr: "127.0.0.1:1".parse().unwrap(),
        p2p_addr: "127.0.0.1:1".parse().unwrap(),
    };
    let mut buf = Vec::new();
    RedbRlp::encode(&peer, &mut buf);
    let mut slice = &buf[..];
    let decoded: PeerEntry = RedbRlp::decode(&mut slice).unwrap();
    assert_eq!(decoded.peer_id.len(), 1000);
}

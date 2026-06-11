use wasix_eth_storage::codecs::{RedbRlp, RlpValue};
use wasix_eth_types::*;
use alloy_primitives::{Address, B256, U256, Bytes};
use alloy_rlp::Encodable;
use redb::Value;

fn test_roundtrip<T: RedbRlp + PartialEq + std::fmt::Debug + Clone>(val: T) {
    // Test RedbRlp directly
    let mut buf = Vec::new();
    RedbRlp::encode(&val, &mut buf);
    assert_eq!(buf.len(), RedbRlp::rlp_length(&val), "Encoded length mismatch for {:?}", val);
    let mut slice = buf.as_slice();
    let decoded = T::decode(&mut slice).expect("Decode failed");
    assert_eq!(val, decoded, "Roundtrip failed for RedbRlp");
    assert!(slice.is_empty(), "Not all bytes consumed for {:?}", val);

    // Test via RlpValue (redb::Value trait)
    let encoded_redb = RlpValue::<T>::as_bytes(&val);
    assert_eq!(buf, encoded_redb, "RedbRlp and RlpValue encoding mismatch");
    let decoded_redb = RlpValue::<T>::from_bytes(&encoded_redb);
    assert_eq!(val, decoded_redb, "Roundtrip failed for RlpValue");
}

#[test]
fn test_u64_codec() {
    test_roundtrip(0u64);
    test_roundtrip(1u64);
    test_roundtrip(u64::MAX);
    
    // Test the fallback to RLP for u64
    // RLP for small u64 (0..=0x7f) is just the byte itself
    let val = 42u64;
    let mut rlp_buf = Vec::new();
    RedbRlp::encode(&val, &mut rlp_buf); // This uses RedbRlp::encode which uses out.put_u64 (BE 8 bytes)
    
    // Wait, let's check u64 RedbRlp implementation again.
    // impl RedbRlp for u64 {
    //     fn encode(&self, out: &mut dyn alloy_rlp::BufMut) { out.put_u64(*self); }
    //     ...
    // }
    // It ALWAYS encodes as 8 bytes BE.
}

#[test]
fn test_string_codec() {
    test_roundtrip("".to_string());
    test_roundtrip("hello".to_string());
    test_roundtrip("🚀".to_string());
}

#[test]
fn test_address_codec() {
    test_roundtrip(Address::ZERO);
    test_roundtrip(Address::repeat_byte(0xff));
}

#[test]
fn test_b256_codec() {
    test_roundtrip(B256::ZERO);
    test_roundtrip(B256::repeat_byte(0xee));
}

#[test]
fn test_u256_codec() {
    test_roundtrip(U256::ZERO);
    test_roundtrip(U256::MAX);
}

#[test]
fn test_bytes_codec() {
    test_roundtrip(Bytes::from(vec![]));
    test_roundtrip(Bytes::from(vec![1, 2, 3, 4]));
}

#[test]
fn test_header_codec() {
    let mut header = Header::default();
    header.number = 123;
    test_roundtrip(header);
}

#[test]
fn test_trie_account_codec() {
    let account = TrieAccount {
        nonce: 10,
        balance: U256::from(1000),
        storage_root: B256::repeat_byte(0x11),
        code_hash: B256::repeat_byte(0x22),
    };
    test_roundtrip(account);
}

#[test]
fn test_payload_id_codec() {
    test_roundtrip(PayloadId::new([1, 2, 3, 4, 5, 6, 7, 8]));
}

#[test]
fn test_address_b256_tuple_codec() {
    test_roundtrip((Address::repeat_byte(0xaa), B256::repeat_byte(0xbb)));
}

#[test]
fn test_account_changeset_codec() {
    let changeset: Vec<(Address, Option<Bytes>)> = vec![
        (Address::repeat_byte(0x1), Some(Bytes::from(vec![0xde, 0xad]))),
        (Address::repeat_byte(0x2), None),
        (Address::repeat_byte(0x3), Some(Bytes::from(vec![]))),
    ];
    test_roundtrip(changeset);
}

#[test]
fn test_storage_changeset_codec() {
    let changeset: Vec<(Address, B256, U256)> = vec![
        (Address::repeat_byte(0x1), B256::repeat_byte(0x2), U256::from(3)),
        (Address::repeat_byte(0x4), B256::repeat_byte(0x5), U256::from(6)),
    ];
    test_roundtrip(changeset);
}

#[test]
fn test_peer_entry_codec() {
    let peer = PeerEntry {
        peer_id: "test_peer".to_string(),
        discovery_addr: "127.0.0.1:30303".parse().unwrap(),
        p2p_addr: "127.0.0.1:30304".parse().unwrap(),
    };
    test_roundtrip(peer);
}

#[test]
fn test_block_receipts_tuple_codec() {
    let block = Block::<Transaction> {
        header: Header::default(),
        body: BlockBody {
            transactions: vec![],
            ommers: vec![],
            withdrawals: None,
        },
    };
    let receipts = vec![Receipt::default()];
    let bundle = BlobsBundleV1::default();
    test_roundtrip((block, receipts, bundle));
}

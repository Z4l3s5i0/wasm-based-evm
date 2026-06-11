#[cfg(test)]
mod tests {
    use alloy_trie::{EMPTY_ROOT_HASH, Nibbles};
    use wasix_eth_storage::EthDatabase;
    use wasix_eth_storage::trie::EthTrie;
    use wasix_eth_storage::write::DatabaseWriteProvider;
    use tempfile::tempdir;

    #[test]
    fn test_branch_terminal_value_manual_nibbles() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = EthDatabase::open(&db_path).unwrap();
        let provider = DatabaseWriteProvider::new(db.inner());
        let batch = provider.begin_batch().unwrap();
        let mut trie = EthTrie::new(&batch, EMPTY_ROOT_HASH);

        // insert("abcd", X)
        // insert("ab",   Y)
        
        let k1 = Nibbles::from_nibbles(vec![0xa, 0xb, 0xc, 0xd]);
        let v1 = vec![0x83, 0x01, 0x02, 0x03];
        let k2 = Nibbles::from_nibbles(vec![0xa, 0xb]);
        let v2 = vec![0x83, 0x04, 0x05, 0x06];

        trie.insert_nibbles(k1.clone(), v1.clone()).unwrap();
        trie.insert_nibbles(k2.clone(), v2.clone()).unwrap();

        assert_eq!(trie.get_nibbles(k1).unwrap(), Some(v1), "Should get v1 for abcd");
        assert_eq!(trie.get_nibbles(k2).unwrap(), Some(v2), "Should get v2 for ab");
    }
    #[test]
    fn test_rlp_encoding_behavior() {
        use alloy_rlp::Encodable;
        use alloy_primitives::Bytes;
        let account_rlp = vec![0xf8, 0x44, 0x01, 0x02]; // Mock RLP list
        let bytes = Bytes::from(account_rlp.clone());
        let mut out = Vec::new();
        bytes.encode(&mut out);
        println!("Bytes Original: {:0x?}", account_rlp);
        println!("Bytes Encoded:  {:0x?}", out);

        use wasix_eth_storage::trie::MyLeafNode;
        use alloy_trie::Nibbles;
        let leaf = MyLeafNode::new(Nibbles::unpack(alloy_primitives::B256::ZERO), account_rlp.clone());
        let mut leaf_out = Vec::new();
        leaf.encode(&mut leaf_out);
        println!("MyLeaf Encoded: {:0x?}", leaf_out);
        let my_root = alloy_primitives::keccak256(&leaf_out);
        println!("MyLeaf Root:    {:0x?}", my_root);

        use alloy_trie::HashBuilder;
        let mut hb = HashBuilder::default();
        hb.add_leaf(Nibbles::unpack(alloy_primitives::B256::ZERO), &account_rlp);
        let hb_root = hb.root();
        println!("HB Root:        {:0x?}", hb_root);

        assert_eq!(my_root, hb_root, "HashBuilder should produce the same root as MyLeafNode");
    }
}

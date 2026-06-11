
#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use alloy_trie::EMPTY_ROOT_HASH;
    use tempfile::tempdir;
    use wasix_eth_storage::EthDatabase;
    use wasix_eth_storage::trie::EthTrie;
    use wasix_eth_storage::write::DatabaseWriteProvider;

    #[test]
    fn test_trie_insert_get() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = EthDatabase::open(&db_path).unwrap();
        let provider = DatabaseWriteProvider::new(db.inner());
        let batch = provider.begin_batch().unwrap();
        let mut trie = EthTrie::new(&batch, EMPTY_ROOT_HASH);

        let key = B256::repeat_byte(0xAA);
        let value = vec![1, 2, 3];

        trie.insert(key, value.clone()).unwrap();
        assert_eq!(trie.get(key).unwrap(), Some(value));
        assert_ne!(trie.root_hash(), EMPTY_ROOT_HASH);
    }

    #[test]
    fn test_trie_prefix_conflict() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_prefix.db");
        let db = EthDatabase::open(&db_path).unwrap();
        let provider = DatabaseWriteProvider::new(db.inner());
        let batch = provider.begin_batch().unwrap();
        let mut trie = EthTrie::new(&batch, EMPTY_ROOT_HASH);

        let mut k1_bytes = [0u8; 32];
        k1_bytes[0] = 0xab;
        k1_bytes[1] = 0xcd;
        let k1 = B256::from(k1_bytes);

        let mut k2_bytes = [0u8; 32];
        k2_bytes[0] = 0xab;
        let k2 = B256::from(k2_bytes);

        let v1 = vec![0x1];
        let v2 = vec![0x2];

        trie.insert(k1, v1.clone()).unwrap();
        trie.insert(k2, v2.clone()).unwrap();

        assert_eq!(trie.get(k1).unwrap(), Some(v1));
        assert_eq!(trie.get(k2).unwrap(), Some(v2));
    }

    #[test]
    fn test_trie_branch_collapse_with_value() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_collapse.db");
        let db = EthDatabase::open(&db_path).unwrap();
        let provider = DatabaseWriteProvider::new(db.inner());
        let batch = provider.begin_batch().unwrap();
        let mut trie = EthTrie::new(&batch, EMPTY_ROOT_HASH);

        let mut k1_bytes = [0u8; 32];
        k1_bytes[0] = 0x12;
        k1_bytes[1] = 0x34;
        let k1 = B256::from(k1_bytes);

        let mut k2_bytes = [0u8; 32];
        k2_bytes[0] = 0x12;
        k2_bytes[1] = 0x56;
        let k2 = B256::from(k2_bytes);

        let mut kp_bytes = [0u8; 32];
        kp_bytes[0] = 0x12;
        let k_prefix = B256::from(kp_bytes);

        let v1 = vec![0x1];
        let v2 = vec![0x2];
        let vp = vec![0x3];

        trie.insert(k1, v1.clone()).unwrap();
        trie.insert(k2, v2.clone()).unwrap();
        trie.insert(k_prefix, vp.clone()).unwrap();

        trie.delete(k1).unwrap();

        assert_eq!(trie.get(k_prefix).unwrap(), Some(vp.clone()));
        assert_eq!(trie.get(k2).unwrap(), Some(v2.clone()));
        assert_eq!(trie.get(k1).unwrap(), None);

        trie.delete(k2).unwrap();
        assert_eq!(trie.get(k_prefix).unwrap(), Some(vp));
        assert_eq!(trie.get(k2).unwrap(), None);
    }

    #[test]
    fn test_trie_delete() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = EthDatabase::open(&db_path).unwrap();
        let provider = DatabaseWriteProvider::new(db.inner());
        let batch = provider.begin_batch().unwrap();
        let mut trie = EthTrie::new(&batch, EMPTY_ROOT_HASH);

        let key = B256::repeat_byte(0xAA);
        let value = vec![1, 2, 3];

        trie.insert(key, value.clone()).unwrap();
        trie.delete(key).unwrap();
        assert_eq!(trie.get(key).unwrap(), None);
        assert_eq!(trie.root_hash(), EMPTY_ROOT_HASH);
    }

    #[test]
    fn test_trie_complex() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = EthDatabase::open(&db_path).unwrap();
        let provider = DatabaseWriteProvider::new(db.inner());
        let batch = provider.begin_batch().unwrap();
        let mut trie = EthTrie::new(&batch, EMPTY_ROOT_HASH);

        let k1 = B256::repeat_byte(0x11);
        let v1 = vec![1];
        let k2 = B256::repeat_byte(0x12);
        let v2 = vec![2];
        let k3 = B256::repeat_byte(0x22);
        let v3 = vec![3];

        trie.insert(k1, v1.clone()).unwrap();
        trie.insert(k2, v2.clone()).unwrap();
        trie.insert(k3, v3.clone()).unwrap();

        assert_eq!(trie.get(k1).unwrap(), Some(v1.clone()));
        assert_eq!(trie.get(k2).unwrap(), Some(v2));
        assert_eq!(trie.get(k3).unwrap(), Some(v3.clone()));

        trie.delete(k2).unwrap();
        assert_eq!(trie.get(k1).unwrap(), Some(v1));
        assert_eq!(trie.get(k2).unwrap(), None);
        assert_eq!(trie.get(k3).unwrap(), Some(v3));
    }
}

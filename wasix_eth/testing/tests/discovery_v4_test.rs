use wasix_eth_p2p::discovery::v4::is_likely_rlp_list;

#[test]
fn test_is_likely_rlp_list() {
    assert!(is_likely_rlp_list(&[0xda]));
    assert!(is_likely_rlp_list(&[0xc0]));
    assert!(is_likely_rlp_list(&[0xf7]));
    assert!(!is_likely_rlp_list(&[0xbf]));
    assert!(!is_likely_rlp_list(&[0x80]));
    assert!(!is_likely_rlp_list(&[]));
}

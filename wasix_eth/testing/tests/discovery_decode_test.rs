use wasix_eth_p2p::discovery::v4::{Ping, NodeEndpoint};
use alloy_rlp::Decodable;
use std::net::IpAddr;
use wasix_eth_types::hex;

#[test]
fn test_decode_ping_log1() {
    let payload_hex = "da04c984ac11000382c99880c984ac11000482232980846a146978";
    let payload = hex::decode(payload_hex).unwrap();
    let mut cursor = &payload[..];
    let ping = Ping::decode(&mut cursor).unwrap();
    assert_eq!(ping.version, 4);
    assert_eq!(ping.from.ip, "172.17.0.3".parse::<IpAddr>().unwrap());
    assert_eq!(ping.from.udp_port, 51608);
    assert_eq!(ping.from.tcp_port, 0);
    assert_eq!(ping.to.ip, "172.17.0.4".parse::<IpAddr>().unwrap());
    assert_eq!(ping.to.udp_port, 9001);
    assert_eq!(ping.to.tcp_port, 0);
}

#[test]
fn test_decode_ping_log2() {
    // Log 2 has IPv6-mapped IPv4
    let payload_hex = "e404c984ac110003828b4480d39000000000000000000000ffffc00002008080846a146978";
    let payload = hex::decode(payload_hex).unwrap();
    let mut cursor = &payload[..];
    let ping = Ping::decode(&mut cursor).unwrap();
    assert_eq!(ping.version, 4);
}

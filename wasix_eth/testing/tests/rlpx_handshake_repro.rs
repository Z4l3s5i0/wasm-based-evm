use tokio::net::{TcpListener, TcpStream};
use k256::SecretKey;
use wasix_eth_p2p::rlpx::stream::RlpxStream;
use wasix_eth_p2p::rlpx::message::Hello;
use wasix_eth_p2p::rlpx::handshake::{ecies, protocol};

#[tokio::test]
async fn test_rlpx_handshake_and_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let initiator_sk = SecretKey::random(&mut rand::thread_rng());
    let recipient_sk = SecretKey::random(&mut rand::thread_rng());
    let recipient_pk = recipient_sk.public_key();

    let initiator_sk_clone = initiator_sk.clone();

    let handle = tokio::spawn(async move {
        let stream = TcpStream::connect(addr).await.unwrap();
        let mut rlpx_stream = RlpxStream::accept(stream).await.unwrap();
        rlpx_stream.initiator = true;
        ecies::do_handshake(&mut rlpx_stream, true, &initiator_sk_clone, Some(&recipient_pk)).await.expect("Initiator ECIES failed");

        let hello = Hello {
            protocol_version: 5,
            client_version: "test-initiator".to_string(),
            capabilities: vec![wasix_eth_types::p2p::Capability { name: "eth".to_string(), version: 68 }],
            listen_port: 0,
            id: wasix_eth_types::B512::ZERO,
        };
        protocol::do_p2p_handshake(&mut rlpx_stream, hello).await.expect("Initiator RLPx handshake failed");
    });

    let (stream, _) = listener.accept().await.unwrap();
    let mut rlpx_stream = RlpxStream::accept(stream).await.unwrap();
    ecies::do_handshake(&mut rlpx_stream, false, &recipient_sk, None).await.expect("Recipient ECIES failed");

    let hello = Hello {
        protocol_version: 5,
        client_version: "test-recipient".to_string(),
        capabilities: vec![wasix_eth_types::p2p::Capability { name: "eth".to_string(), version: 68 }],
        listen_port: 0,
        id: wasix_eth_types::B512::ZERO,
    };

    // This is where it should fail if there is a MAC mismatch
    protocol::do_p2p_handshake(&mut rlpx_stream, hello).await.expect("Recipient RLPx handshake failed");

    handle.await.unwrap();
}

use std::sync::Arc;
use std::time::Duration;

use p2pchat_signaling::SignalingHub;
use p2pchat_transport::mock::MockFactory;
use p2pchat_transport::{Transport, WebRtcTransport};

/// Ждём, пока `send` перестанет падать с «not connected».
async fn send_with_retry(t: &WebRtcTransport, to: &[u8], data: &[u8]) {
    for _ in 0..50 {
        if t.send(&to.to_vec(), data).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("connection never established");
}

#[tokio::test]
async fn two_peers_exchange_via_signaling_and_webrtc() {
    let hub = SignalingHub::new();
    let a_sig: Arc<dyn p2pchat_signaling::Signaling> =
        Arc::new(hub.connect(b"alice".to_vec()).await);
    let b_sig: Arc<dyn p2pchat_signaling::Signaling> = Arc::new(hub.connect(b"bob".to_vec()).await);

    let factory = Arc::new(MockFactory::new());
    let alice = WebRtcTransport::new(b"alice".to_vec(), a_sig, factory.clone());
    let bob = WebRtcTransport::new(b"bob".to_vec(), b_sig, factory.clone());

    let mut alice_in = alice.incoming().await;
    let mut bob_in = bob.incoming().await;

    // Alice инициирует соединение с Bob.
    alice.dial(b"bob".to_vec()).await.unwrap();

    // Дождавшись установления, шлём данные в обе стороны.
    send_with_retry(&alice, b"bob", b"ping").await;
    let got = tokio::time::timeout(Duration::from_secs(2), bob_in.recv())
        .await
        .expect("no inbound on bob")
        .expect("channel closed");
    assert_eq!(got.from, b"alice".to_vec());
    assert_eq!(got.data, b"ping");

    send_with_retry(&bob, b"alice", b"pong").await;
    let got = tokio::time::timeout(Duration::from_secs(2), alice_in.recv())
        .await
        .expect("no inbound on alice")
        .expect("channel closed");
    assert_eq!(got.from, b"bob".to_vec());
    assert_eq!(got.data, b"pong");
}

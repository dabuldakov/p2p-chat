//! Проверка задеплоенного signaling-сервера.
//!   cargo run -p p2pchat-signal-server --example remote_check -- ws://host:port

use std::time::Duration;

use p2pchat_proto::signal_envelope::Body;
use p2pchat_proto::SignalEnvelope;
use p2pchat_signaling::{answer, offer, register, Signaling, WsSignaling};
use tokio::sync::mpsc;

async fn recv_signal(rx: &mut mpsc::Receiver<SignalEnvelope>) -> SignalEnvelope {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout")
            .expect("closed");
        if !matches!(msg.body, Some(Body::Register(_))) {
            return msg;
        }
    }
}

#[tokio::main]
async fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "ws://90.188.89.63:8090".to_string());
    println!("connecting to {url}");

    let a_pk = b"remote-check-a".to_vec();
    let b_pk = b"remote-check-b".to_vec();

    let a = WsSignaling::connect(&url, register(a_pk.clone(), a_pk.clone(), vec![]))
        .await
        .expect("A connect");
    let b = WsSignaling::connect(&url, register(b_pk.clone(), b_pk.clone(), vec![]))
        .await
        .expect("B connect");

    let mut a_rx = a.incoming().await;
    let mut b_rx = b.incoming().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    a.send(offer(
        a_pk.clone(),
        b_pk.clone(),
        b"SDP-OFFER".to_vec(),
        "s1".into(),
    ))
    .await
    .expect("send offer");
    let got = recv_signal(&mut b_rx).await;
    assert_eq!(got.to, b_pk, "offer должен прийти B");
    println!("B <- offer от {}", hex4(&got.from));

    b.send(answer(
        b_pk.clone(),
        a_pk.clone(),
        b"SDP-ANSWER".to_vec(),
        "s1".into(),
    ))
    .await
    .expect("send answer");
    let got = recv_signal(&mut a_rx).await;
    assert_eq!(got.to, a_pk, "answer должен прийти A");
    println!("A <- answer от {}", hex4(&got.from));

    println!("OK: маршрутизация через {url} работает");
}

fn hex4(b: &[u8]) -> String {
    b.iter().take(4).map(|x| format!("{:02x}", x)).collect()
}

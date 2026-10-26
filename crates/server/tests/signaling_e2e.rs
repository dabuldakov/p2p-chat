//! End-to-end: реальный signaling-сервер по WebSocket + два клиента.

use std::time::Duration;

use p2pchat_proto::signal_envelope::Body;
use p2pchat_signaling::{offer, register, Signaling, WsSignaling};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// Ждём первое не-`Register` сообщение (регистрации рассылаются всем).
async fn recv_signal(
    rx: &mut mpsc::Receiver<p2pchat_proto::SignalEnvelope>,
) -> p2pchat_proto::SignalEnvelope {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timeout waiting for signal")
            .expect("channel open");
        if !matches!(msg.body, Some(Body::Register(_))) {
            return msg;
        }
    }
}

#[tokio::test]
async fn ws_signaling_routes_offer_end_to_end() {
    // поднимаем сервер на эфемерном порту
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = p2pchat_signal_server::serve_listener(listener).await;
    });
    let url = format!("ws://{addr}");

    let pk_a = b"alice".to_vec();
    let pk_b = b"bob".to_vec();

    // подключаемся и регистрируемся
    let a = WsSignaling::connect(&url, register(pk_a.clone(), pk_a.clone(), vec![]))
        .await
        .unwrap();
    let b = WsSignaling::connect(&url, register(pk_b.clone(), pk_b.clone(), vec![]))
        .await
        .unwrap();

    let mut b_rx = b.incoming().await;
    let mut a_rx = a.incoming().await;

    // даём серверу обработать регистрацию
    tokio::time::sleep(Duration::from_millis(150)).await;

    // A шлёт offer для B
    a.send(offer(
        pk_a.clone(),
        pk_b.clone(),
        b"sdp-offer".to_vec(),
        "s1".into(),
    ))
    .await
    .unwrap();

    let got = recv_signal(&mut b_rx).await;
    assert_eq!(got.from, pk_a);
    assert_eq!(got.to, pk_b);

    // B отвечает answer для A
    b.send(p2pchat_signaling::answer(
        pk_b.clone(),
        pk_a.clone(),
        b"sdp-answer".to_vec(),
        "s1".into(),
    ))
    .await
    .unwrap();

    let got = recv_signal(&mut a_rx).await;
    assert_eq!(got.from, pk_b);
    assert_eq!(got.to, pk_a);
}

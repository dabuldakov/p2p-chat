//! Логика signaling-сервера (вынесена в lib для тестов).

pub mod report;
pub mod turn;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use p2pchat_proto::signal_envelope::Body;
use p2pchat_proto::SignalEnvelope;

pub type Clients = Arc<Mutex<HashMap<Vec<u8>, mpsc::Sender<Vec<u8>>>>>;

/// Принимать соединения, пока слушатель жив.
pub async fn serve_listener(listener: TcpListener) -> Result<()> {
    let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
    loop {
        let (stream, addr) = listener.accept().await?;
        let clients = clients.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, clients).await {
                warn!("{addr}: {e}");
            }
        });
    }
}

pub async fn serve(addr: &str) -> Result<()> {
    let listener = TcpListener::bind(addr).await?;
    info!("signaling server listening on {}", listener.local_addr()?);
    serve_listener(listener).await
}

async fn handle(stream: TcpStream, clients: Clients) -> Result<()> {
    let ws = accept_async(stream).await?;
    let (mut write, mut read) = ws.split();

    let (tx, mut rx) = mpsc::channel::<Vec<u8>>(256);
    let writer = tokio::spawn(async move {
        while let Some(b) = rx.recv().await {
            if write.send(Message::Binary(b)).await.is_err() {
                break;
            }
        }
    });

    let mut my_key: Option<Vec<u8>> = None;
    while let Some(msg) = read.next().await {
        let msg = msg?;
        let Message::Binary(b) = msg else { continue };
        let Ok(env) = SignalEnvelope::decode(b.as_ref()) else {
            continue;
        };
        match &env.body {
            Some(Body::Register(_)) => {
                my_key = Some(env.from.clone());
                clients.lock().unwrap().insert(env.from.clone(), tx.clone());
                info!("registered {}", hex4(&env.from));
                broadcast(&clients, &env).await;
            }
            Some(_) => route(&clients, &env).await,
            None => {}
        }
    }

    if let Some(k) = my_key {
        clients.lock().unwrap().remove(&k);
        info!("disconnected {}", hex4(&k));
    }
    writer.abort();
    Ok(())
}

/// `Register` — рассылаем всем, кроме отправителя: узлы узнают друг друга.
async fn broadcast(clients: &Clients, env: &SignalEnvelope) {
    let bytes = env.encode_to_vec();
    let targets: Vec<mpsc::Sender<Vec<u8>>> = {
        let map = clients.lock().unwrap();
        map.iter()
            .filter(|(k, _)| **k != env.from)
            .map(|(_, tx)| tx.clone())
            .collect()
    };
    for tx in targets {
        let _ = tx.send(bytes.clone()).await;
    }
}

/// `Offer`/`Answer`/`Ice` — строго адресату.
async fn route(clients: &Clients, env: &SignalEnvelope) {
    let tx = { clients.lock().unwrap().get(&env.to).cloned() };
    match tx {
        Some(tx) => {
            let _ = tx.send(env.encode_to_vec()).await;
        }
        None => warn!("no route to {}", hex4(&env.to)),
    }
}

fn hex4(bytes: &[u8]) -> String {
    bytes.iter().take(4).map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use p2pchat_proto::signal_envelope::Body;
    use p2pchat_proto::{Offer, Register};
    use tokio::sync::mpsc;

    fn offer_env(from: &[u8], to: &[u8]) -> SignalEnvelope {
        SignalEnvelope {
            from: from.to_vec(),
            to: to.to_vec(),
            body: Some(Body::Offer(Offer {
                sdp: b"sdp".to_vec(),
                session_id: "s".into(),
            })),
        }
    }

    #[tokio::test]
    async fn route_delivers_only_to_target() {
        let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
        let (txa, mut rxa) = mpsc::channel(8);
        let (txb, mut rxb) = mpsc::channel(8);
        clients.lock().unwrap().insert(b"a".to_vec(), txa);
        clients.lock().unwrap().insert(b"b".to_vec(), txb);

        route(&clients, &offer_env(b"a", b"b")).await;
        assert!(rxb.recv().await.is_some());
        assert!(rxa.try_recv().is_err(), "источнику не дублируем");
    }

    #[tokio::test]
    async fn route_unknown_target_is_noop() {
        let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
        route(&clients, &offer_env(b"a", b"zzz")).await;
    }

    #[tokio::test]
    async fn broadcast_skips_sender() {
        let clients: Clients = Arc::new(Mutex::new(HashMap::new()));
        let (txa, mut rxa) = mpsc::channel(8);
        let (txb, mut rxb) = mpsc::channel(8);
        clients.lock().unwrap().insert(b"a".to_vec(), txa);
        clients.lock().unwrap().insert(b"b".to_vec(), txb);
        let env = SignalEnvelope {
            from: b"a".to_vec(),
            to: Vec::new(),
            body: Some(Body::Register(Register {
                peer_id: b"a".to_vec(),
                endpoints: vec![],
                is_relay: false,
            })),
        };
        broadcast(&clients, &env).await;
        assert!(rxb.recv().await.is_some());
        assert!(rxa.try_recv().is_err());
    }

    #[test]
    fn hex4_formats_prefix() {
        assert_eq!(hex4(&[0xde, 0xad, 0xbe, 0xef, 1]), "deadbeef");
        assert_eq!(hex4(&[1]), "01");
    }
}

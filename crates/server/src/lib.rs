//! Логика signaling-сервера (вынесена в lib для тестов).

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
            if write.send(Message::Binary(b.into())).await.is_err() {
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
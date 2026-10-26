//! WebSocket-клиент signaling.
//!
//! Протокол на проводе: каждый кадр — сериализованный `SignalEnvelope`.
//! Первым сообщением клиент обязан отправить `Register` (там его публичный
//! ключ), после чего сервер маршрутизирует `Offer`/`Answer`/`Ice`.

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use p2pchat_proto::SignalEnvelope;

use crate::{Signaling, SignalingError};

pub struct WsSignaling {
    tx: mpsc::Sender<Vec<u8>>,
    rx: Mutex<Option<mpsc::Receiver<SignalEnvelope>>>,
}

impl WsSignaling {
    /// Подключиться и сразу отправить `Register` (идентификация узла).
    pub async fn connect(url: &str, register: SignalEnvelope) -> Result<Self, SignalingError> {
        let (stream, _) = connect_async(url)
            .await
            .map_err(|e| SignalingError::Protocol(format!("connect: {e}")))?;
        let (mut write, mut read) = stream.split();

        write
            .send(Message::Binary(register.encode_to_vec()))
            .await
            .map_err(|e| SignalingError::Protocol(format!("register: {e}")))?;

        let (tx, mut trx) = mpsc::channel::<Vec<u8>>(256);
        tokio::spawn(async move {
            while let Some(bytes) = trx.recv().await {
                if write.send(Message::Binary(bytes)).await.is_err() {
                    break;
                }
            }
        });

        let (in_tx, in_rx) = mpsc::channel::<SignalEnvelope>(256);
        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                if let Message::Binary(b) = msg {
                    if let Ok(env) = SignalEnvelope::decode(b.as_ref()) {
                        if in_tx.send(env).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            tx,
            rx: Mutex::new(Some(in_rx)),
        })
    }
}

#[async_trait]
impl Signaling for WsSignaling {
    async fn send(&self, msg: SignalEnvelope) -> Result<(), SignalingError> {
        self.tx
            .send(msg.encode_to_vec())
            .await
            .map_err(|_| SignalingError::Closed)
    }

    async fn incoming(&self) -> mpsc::Receiver<SignalEnvelope> {
        self.rx
            .lock()
            .await
            .take()
            .expect("incoming() must be called once")
    }
}

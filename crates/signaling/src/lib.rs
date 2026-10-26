//! Signaling: обмен `Offer`/`Answer`/`IceCandidate` между узлами через
//! инфраструктурный сервер. Нужен **только** для установки WebRTC-соединения;
//! чат через него не идёт.
//!
//! * [`Signaling`] — абстракция клиента (отправить / получить).
//! * [`SignalingHub`] + [`MemorySignaling`] — in-memory реализация для тестов.
//! * [`ws::WsSignaling`] — реальный клиент поверх WebSocket.

pub mod ws;

pub use ws::WsSignaling;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use p2pchat_proto::signal_envelope::Body;
use p2pchat_proto::SignalEnvelope;
use thiserror::Error;
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Error)]
pub enum SignalingError {
    #[error("peer not registered: {0:?}")]
    UnknownPeer(Vec<u8>),
    #[error("channel closed")]
    Closed,
    #[error("protocol: {0}")]
    Protocol(String),
}

#[async_trait]
pub trait Signaling: Send + Sync {
    /// Отправить сигнальное сообщение (маршрутизация по `to`).
    async fn send(&self, msg: SignalEnvelope) -> Result<(), SignalingError>;

    /// Поток входящих сигнальных сообщений (один раз при старте).
    async fn incoming(&self) -> mpsc::Receiver<SignalEnvelope>;
}

/// In-memory «сервер знакомств»: держит каналы подключённых узлов.
#[derive(Clone, Default)]
pub struct SignalingHub {
    clients: Arc<Mutex<HashMap<Vec<u8>, mpsc::Sender<SignalEnvelope>>>>,
}

impl SignalingHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Подключить узел; возвращает его клиент.
    pub async fn connect(&self, public_key: Vec<u8>) -> MemorySignaling {
        let (tx, rx) = mpsc::channel(256);
        self.clients.lock().await.insert(public_key.clone(), tx);
        MemorySignaling {
            hub: self.clone(),
            me: public_key,
            rx: Mutex::new(Some(rx)),
        }
    }

    /// Маршрутизация. `Register` рассылается всем (узлы узнают друг друга),
    /// остальное — строго адресату.
    async fn route(&self, msg: &SignalEnvelope) -> Result<(), SignalingError> {
        let clients = self.clients.lock().await;
        match &msg.body {
            Some(Body::Register(_)) => {
                for (key, tx) in clients.iter() {
                    if key != &msg.from {
                        let _ = tx.send(msg.clone()).await;
                    }
                }
            }
            Some(_) => {
                let tx = clients
                    .get(&msg.to)
                    .ok_or_else(|| SignalingError::UnknownPeer(msg.to.clone()))?;
                tx.send(msg.clone())
                    .await
                    .map_err(|_| SignalingError::Closed)?;
            }
            None => return Err(SignalingError::Protocol("empty body".into())),
        }
        Ok(())
    }
}

/// Клиент in-memory signaling.
pub struct MemorySignaling {
    hub: SignalingHub,
    me: Vec<u8>,
    rx: Mutex<Option<mpsc::Receiver<SignalEnvelope>>>,
}

impl MemorySignaling {
    pub fn public_key(&self) -> &[u8] {
        &self.me
    }

    pub fn hub(&self) -> &SignalingHub {
        &self.hub
    }
}

#[async_trait]
impl Signaling for MemorySignaling {
    async fn send(&self, msg: SignalEnvelope) -> Result<(), SignalingError> {
        self.hub.route(&msg).await
    }

    async fn incoming(&self) -> mpsc::Receiver<SignalEnvelope> {
        self.rx
            .lock()
            .await
            .take()
            .expect("incoming() must be called once")
    }
}

/// Хелперы для сборки сигнальных сообщений.
pub fn register(
    from: Vec<u8>,
    peer_id: Vec<u8>,
    endpoints: Vec<p2pchat_proto::Endpoint>,
) -> SignalEnvelope {
    SignalEnvelope {
        from,
        to: Vec::new(),
        body: Some(Body::Register(p2pchat_proto::Register {
            peer_id,
            endpoints,
            is_relay: false,
        })),
    }
}

pub fn offer(from: Vec<u8>, to: Vec<u8>, sdp: Vec<u8>, session_id: String) -> SignalEnvelope {
    SignalEnvelope {
        from,
        to,
        body: Some(Body::Offer(p2pchat_proto::Offer { sdp, session_id })),
    }
}

pub fn answer(from: Vec<u8>, to: Vec<u8>, sdp: Vec<u8>, session_id: String) -> SignalEnvelope {
    SignalEnvelope {
        from,
        to,
        body: Some(Body::Answer(p2pchat_proto::Answer { sdp, session_id })),
    }
}

pub fn ice(
    from: Vec<u8>,
    to: Vec<u8>,
    candidate: String,
    sdp_mid: String,
    mline: u32,
) -> SignalEnvelope {
    SignalEnvelope {
        from,
        to,
        body: Some(Body::Ice(p2pchat_proto::IceCandidate {
            candidate,
            sdp_mid,
            sdp_mline_index: mline,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn routes_offer_to_target_only() {
        let hub = SignalingHub::new();
        let a = hub.connect(b"a".to_vec()).await;
        let b = hub.connect(b"b".to_vec()).await;
        let c = hub.connect(b"c".to_vec()).await;

        let mut b_rx = b.incoming().await;
        let mut c_rx = c.incoming().await;

        a.send(offer(
            b"a".to_vec(),
            b"b".to_vec(),
            b"sdp".to_vec(),
            "s1".into(),
        ))
        .await
        .unwrap();

        let got = b_rx.try_recv().expect("b must receive");
        assert_eq!(got.to, b"b".to_vec());
        assert!(c_rx.try_recv().is_err(), "c must not receive");
    }

    #[tokio::test]
    async fn register_is_broadcast() {
        let hub = SignalingHub::new();
        let a = hub.connect(b"a".to_vec()).await;
        let b = hub.connect(b"b".to_vec()).await;
        let mut b_rx = b.incoming().await;

        a.send(register(b"a".to_vec(), b"a".to_vec(), vec![]))
            .await
            .unwrap();
        let got = b_rx.try_recv().expect("broadcast");
        assert_eq!(got.from, b"a".to_vec());
    }
}

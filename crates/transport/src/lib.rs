//! Транспортный слой.
//!
//! `core` умеет только превращать действия в байты и обратно. Всё, что
//! касается сети — здесь. Интерфейс [`Transport`] намеренно узкий:
//! «отправь байты этому peer_id» и «дай поток входящих».
//!
//! Реализации:
//! * [`memory`] — в памяти, для тестов.
//! * [`webrtc`] — оркестрация WebRTC через signaling; сама реализация
//!   `PeerConnection` приходит от платформы (Android / webrtc-rs).

pub mod memory;
pub mod mock;
pub mod webrtc;

pub use memory::{MemoryHub, MemoryTransport};
pub use webrtc::{PeerConnection, PeerConnectionFactory, PeerEvent, WebRtcTransport};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

/// Идентификатор узла (в транспорте — публичный ключ, т.к. signaling
/// маршрутизирует по ключам).
pub type PeerId = Vec<u8>;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("peer unreachable: {0:?}")]
    Unreachable(PeerId),
    #[error("channel closed")]
    Closed,
    #[error("webrtc: {0}")]
    WebRtc(String),
}

/// Входящее сообщение: от кого и сырые байты конверта.
#[derive(Debug, Clone)]
pub struct Inbound {
    pub from: PeerId,
    pub data: Vec<u8>,
}

#[async_trait]
pub trait Transport: Send + Sync {
    /// Отправить конверт конкретному узлу.
    async fn send(&self, to: &PeerId, data: &[u8]) -> Result<(), TransportError>;

    /// Поток входящих сообщений. Вызывается один раз при старте.
    async fn incoming(&self) -> mpsc::Receiver<Inbound>;
}
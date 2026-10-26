//! Mock-реализация WebRTC для тестов оркестрации.
//!
//! Не передаёт настоящий SDP/ICE — «SDP» здесь это публичный ключ пира,
//! а «соединение» считается установленным сразу после обмена. Зато
//! полностью проверяется логика: кто шлёт offer, кто answer, как
//! гоняются ICE и как данные доходят до `Inbound`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

use crate::webrtc::{PeerConnection, PeerConnectionFactory, PeerEvent};
use crate::{PeerId, TransportError};

/// Фабрика mock-соединений с общим реестром, чтобы пиры находили друг друга.
#[derive(Clone, Default)]
pub struct MockFactory {
    registry: Arc<Mutex<HashMap<PeerId, Arc<MockPeerConnection>>>>,
}

impl MockFactory {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl PeerConnectionFactory for MockFactory {
    async fn create(&self, local_pk: PeerId) -> Result<Arc<dyn PeerConnection>, TransportError> {
        let (tx, rx) = mpsc::channel(256);
        let conn = Arc::new(MockPeerConnection {
            local: local_pk.clone(),
            registry: self.registry.clone(),
            remote: AsyncMutex::new(None),
            events_tx: tx,
            events_rx: AsyncMutex::new(Some(rx)),
        });
        self.registry.lock().unwrap().insert(local_pk, conn.clone());
        Ok(conn)
    }
}

pub struct MockPeerConnection {
    local: PeerId,
    registry: Arc<Mutex<HashMap<PeerId, Arc<MockPeerConnection>>>>,
    remote: AsyncMutex<Option<Arc<MockPeerConnection>>>,
    events_tx: mpsc::Sender<PeerEvent>,
    events_rx: AsyncMutex<Option<mpsc::Receiver<PeerEvent>>>,
}

impl MockPeerConnection {
    /// Доставить данные в этот узел (вызывается «сетевым» пиром).
    async fn deliver(&self, data: Vec<u8>) {
        let _ = self.events_tx.send(PeerEvent::Data(data)).await;
    }

    async fn link(&self, remote_pk: &[u8]) {
        let remote = self.registry.lock().unwrap().get(remote_pk).cloned();
        *self.remote.lock().await = remote;
    }

    async fn emit_ice(&self) {
        let _ = self
            .events_tx
            .send(PeerEvent::IceCandidate {
                candidate: format!("candidate-{}", hex(&self.local)),
                sdp_mid: "0".into(),
                mline: 0,
            })
            .await;
    }
}

#[async_trait]
impl PeerConnection for MockPeerConnection {
    async fn create_offer(&self) -> Result<Vec<u8>, TransportError> {
        self.emit_ice().await;
        let _ = self.events_tx.send(PeerEvent::Connected).await;
        Ok(self.local.clone())
    }

    async fn create_answer(&self) -> Result<Vec<u8>, TransportError> {
        self.emit_ice().await;
        let _ = self.events_tx.send(PeerEvent::Connected).await;
        Ok(self.local.clone())
    }

    async fn set_remote_description(&self, sdp: &[u8]) -> Result<(), TransportError> {
        self.link(sdp).await;
        Ok(())
    }

    async fn add_ice_candidate(
        &self,
        _candidate: &str,
        _sdp_mid: &str,
        _mline: u32,
    ) -> Result<(), TransportError> {
        Ok(())
    }

    async fn send_data(&self, data: &[u8]) -> Result<(), TransportError> {
        let remote = self.remote.lock().await.clone();
        match remote {
            Some(remote) => {
                remote.deliver(data.to_vec()).await;
                Ok(())
            }
            None => Err(TransportError::WebRtc("not connected".into())),
        }
    }

    async fn events(&self) -> mpsc::Receiver<PeerEvent> {
        self.events_rx
            .lock()
            .await
            .take()
            .expect("events() must be called once")
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

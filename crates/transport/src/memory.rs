//! Транспорт в памяти: прогоняет gossip-логику без реальной сети.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::{Inbound, PeerId, Transport, TransportError};

/// Общий «коммутатор» для набора узлов в памяти.
#[derive(Clone, Default)]
pub struct MemoryHub {
    registry: Arc<Mutex<HashMap<PeerId, mpsc::Sender<Inbound>>>>,
}

impl MemoryHub {
    pub fn new() -> Self {
        Self::default()
    }

    fn register(&self, id: PeerId, tx: mpsc::Sender<Inbound>) {
        self.registry.lock().unwrap().insert(id, tx);
    }

    fn lookup(&self, id: &PeerId) -> Option<mpsc::Sender<Inbound>> {
        self.registry.lock().unwrap().get(id).cloned()
    }
}

/// Транспорт в памяти.
pub struct MemoryTransport {
    hub: MemoryHub,
    me: PeerId,
    rx: Mutex<Option<mpsc::Receiver<Inbound>>>,
}

impl MemoryTransport {
    /// Создать узел и зарегистрировать его в общем хабе.
    pub fn join(hub: &MemoryHub, me: PeerId) -> Self {
        let (tx, rx) = mpsc::channel(1024);
        hub.register(me.clone(), tx);
        Self { hub: hub.clone(), me, rx: Mutex::new(Some(rx)) }
    }
}

#[async_trait]
impl Transport for MemoryTransport {
    async fn send(&self, to: &PeerId, data: &[u8]) -> Result<(), TransportError> {
        let tx = self.hub.lookup(to).ok_or_else(|| TransportError::Unreachable(to.clone()))?;
        tx.send(Inbound { from: self.me.clone(), data: data.to_vec() })
            .await
            .map_err(|_| TransportError::Closed)
    }

    async fn incoming(&self) -> mpsc::Receiver<Inbound> {
        self.rx
            .lock()
            .unwrap()
            .take()
            .expect("incoming() must be called once")
    }
}
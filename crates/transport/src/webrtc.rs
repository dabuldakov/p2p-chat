//! Оркестрация WebRTC-соединений поверх signaling.
//!
//! Этот модуль **не реализует сам WebRTC** — он описывает, как им
//! пользоваться, и выполняет рукопожатие:
//!
//! 1. Диалер: создаёт `PeerConnection`, offer → signaling → ответчик.
//! 2. Ответчик: создаёт `PeerConnection`, answer → signaling → диалер.
//! 3. Обе стороны гоняют ICE-кандидатов через signaling.
//! 4. Открывается DataChannel — `Data`-события превращаются в [`Inbound`].
//!
//! Реальную реализацию [`PeerConnection`] поставляет платформа:
//! Android — Google WebRTC SDK (Kotlin), узел — `webrtc-rs` или TCP.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use p2pchat_proto::signal_envelope::Body;
use p2pchat_signaling::{answer, ice, offer, Signaling};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, warn};

use crate::{Inbound, PeerId, Transport, TransportError};

/// События, которые [`PeerConnection`] отдаёт наружу.
#[derive(Debug, Clone)]
pub enum PeerEvent {
    /// Локальный ICE-кандидат — надо отправить собеседнику через signaling.
    IceCandidate { candidate: String, sdp_mid: String, mline: u32 },
    /// Пришли данные из DataChannel.
    Data(Vec<u8>),
    /// Соединение установлено.
    Connected,
    /// Соединение закрыто.
    Closed,
}

/// Платформенная реализация одного WebRTC-соединения.
#[async_trait]
pub trait PeerConnection: Send + Sync {
    async fn create_offer(&self) -> Result<Vec<u8>, TransportError>;
    async fn create_answer(&self) -> Result<Vec<u8>, TransportError>;
    async fn set_remote_description(&self, sdp: &[u8]) -> Result<(), TransportError>;
    async fn add_ice_candidate(
        &self,
        candidate: &str,
        sdp_mid: &str,
        mline: u32,
    ) -> Result<(), TransportError>;
    async fn send_data(&self, data: &[u8]) -> Result<(), TransportError>;
    /// Поток событий соединения (один раз при старте).
    async fn events(&self) -> mpsc::Receiver<PeerEvent>;
}

/// Фабрика соединений. `local_pk` — наша идентичность (нужна для SDP/handshake).
#[async_trait]
pub trait PeerConnectionFactory: Send + Sync {
    async fn create(&self, local_pk: PeerId) -> Result<Arc<dyn PeerConnection>, TransportError>;
}

struct Inner {
    me: PeerId,
    signaling: Arc<dyn Signaling>,
    factory: Arc<dyn PeerConnectionFactory>,
    conns: Mutex<HashMap<PeerId, Arc<dyn PeerConnection>>>,
    incoming_tx: mpsc::Sender<Inbound>,
}

/// Транспорт на WebRTC.
pub struct WebRtcTransport {
    inner: Arc<Inner>,
    rx: Mutex<Option<mpsc::Receiver<Inbound>>>,
}

impl WebRtcTransport {
    /// Создать транспорт и запустить цикл обработки signaling.
    pub fn new(
        me: PeerId,
        signaling: Arc<dyn Signaling>,
        factory: Arc<dyn PeerConnectionFactory>,
    ) -> Arc<Self> {
        let (incoming_tx, incoming_rx) = mpsc::channel(1024);
        let inner = Arc::new(Inner {
            me,
            signaling,
            factory,
            conns: Mutex::new(HashMap::new()),
            incoming_tx,
        });
        let transport = Arc::new(Self { inner: inner.clone(), rx: Mutex::new(Some(incoming_rx)) });

        // цикл signaling
        tokio::spawn(async move {
            let mut sig_rx = inner.signaling.incoming().await;
            while let Some(msg) = sig_rx.recv().await {
                if let Err(e) = handle_signal(&inner, msg).await {
                    warn!("signal: {e}");
                }
            }
        });

        transport
    }

    /// Инициировать соединение с узлом `peer` (по его публичному ключу).
    pub async fn dial(&self, peer: PeerId) -> Result<(), TransportError> {
        let pc = self.inner.factory.create(self.inner.me.clone()).await?;
        let from = peer.clone();
        let inner = self.inner.clone();
        spawn_event_pump(inner, from, pc.clone());

        let sdp = pc.create_offer().await?;
        self.inner.conns.lock().await.insert(peer.clone(), pc);
        self.inner
            .signaling
            .send(offer(
                self.inner.me.clone(),
                peer,
                sdp,
                uuid_like(),
            ))
            .await
            .map_err(|e| TransportError::WebRtc(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl Transport for WebRtcTransport {
    async fn send(&self, to: &PeerId, data: &[u8]) -> Result<(), TransportError> {
        let conns = self.inner.conns.lock().await;
        let pc = conns.get(to).ok_or_else(|| TransportError::Unreachable(to.clone()))?;
        pc.send_data(data).await
    }

    async fn incoming(&self) -> mpsc::Receiver<Inbound> {
        self.rx
            .lock()
            .await
            .take()
            .expect("incoming() must be called once")
    }
}

async fn handle_signal(
    inner: &Arc<Inner>,
    msg: p2pchat_proto::SignalEnvelope,
) -> Result<(), TransportError> {
    let from = msg.from.clone();
    match msg.body {
        Some(Body::Offer(o)) => {
            debug!("<- offer from {:?}", from);
            let pc = inner.factory.create(inner.me.clone()).await?;
            spawn_event_pump(inner.clone(), from.clone(), pc.clone());
            inner.conns.lock().await.insert(from.clone(), pc.clone());
            pc.set_remote_description(&o.sdp).await?;
            let sdp = pc.create_answer().await?;
            inner
                .signaling
                .send(answer(inner.me.clone(), from, sdp, o.session_id))
                .await
                .map_err(|e| TransportError::WebRtc(e.to_string()))?;
        }
        Some(Body::Answer(a)) => {
            debug!("<- answer from {:?}", from);
            if let Some(pc) = inner.conns.lock().await.get(&from).cloned() {
                pc.set_remote_description(&a.sdp).await?;
            }
        }
        Some(Body::Ice(c)) => {
            if let Some(pc) = inner.conns.lock().await.get(&from).cloned() {
                pc.add_ice_candidate(&c.candidate, &c.sdp_mid, c.sdp_mline_index)
                    .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn spawn_event_pump(inner: Arc<Inner>, from: PeerId, pc: Arc<dyn PeerConnection>) {
    tokio::spawn(async move {
        let mut events = pc.events().await;
        while let Some(ev) = events.recv().await {
            match ev {
                PeerEvent::IceCandidate { candidate, sdp_mid, mline } => {
                    let _ = inner
                        .signaling
                        .send(ice(inner.me.clone(), from.clone(), candidate, sdp_mid, mline))
                        .await;
                }
                PeerEvent::Data(data) => {
                    let _ = inner
                        .incoming_tx
                        .send(Inbound { from: from.clone(), data })
                        .await;
                }
                PeerEvent::Connected => debug!("connected to {:?}", from),
                PeerEvent::Closed => break,
            }
        }
    });
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("s{n:x}")
}
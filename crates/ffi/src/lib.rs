//! UniFFI-мост: тонкая обёртка над `core` для Kotlin/Android.
//!
//! Из Rust наружу торчит минимум: создать ядро, отдать/принять байты,
//! получить историю. Вся работа с WebRTC и UI — на стороне Kotlin.
//!
//! Генерация Kotlin-биндингов:
//! `cargo run -p p2pchat-ffi --bin uniffi-bindgen generate ...`
//! (добавим позже; сейчас важна компилируемость и форма API).

uniffi::setup_scaffolding!();

use std::sync::{Arc, Mutex};

use p2pchat_core::{GossipEngine, GroupId, Log};

/// Ошибка на границе FFI (UniFFI сам превратит в Kotlin-исключение).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("{reason}")]
    Core { reason: String },
}

impl From<p2pchat_core::CoreError> for FfiError {
    fn from(e: p2pchat_core::CoreError) -> Self {
        FfiError::Core { reason: e.to_string() }
    }
}

/// Одно сообщение для UI.
#[derive(uniffi::Record)]
pub struct FfiMessage {
    pub id: Vec<u8>,
    pub group_id: Vec<u8>,
    pub author: Vec<u8>,
    pub text: String,
    pub lamport: u64,
    pub wall_time: u64,
}

/// Пир для UI.
#[derive(uniffi::Record)]
pub struct FfiPeer {
    pub peer_id: Vec<u8>,
    pub last_seen: u64,
    pub is_relay: bool,
}

/// Ядро чата, которым владеет Android-слой.
#[derive(uniffi::Object)]
pub struct ChatCore {
    inner: Mutex<GossipEngine>,
}

#[uniffi::export]
impl ChatCore {
    /// Создать ядро. `db_path` — файл SQLite (или ":memory:").
    #[uniffi::constructor]
    pub fn new(db_path: String) -> Result<Arc<Self>, FfiError> {
        let identity = p2pchat_crypto::Identity::generate();
        let store = if db_path == ":memory:" {
            p2pchat_storage::Store::open_in_memory()
        } else {
            p2pchat_storage::Store::open(&db_path)
        }
        .map_err(|e| FfiError::Core { reason: e.to_string() })?;

        let log = Log::with_store(store)?;
        let engine = GossipEngine::new(identity, log);
        Ok(Arc::new(Self { inner: Mutex::new(engine) }))
    }

    /// Наш peer_id (hex-строкой — удобно для UI и логов).
    pub fn peer_id_hex(&self) -> String {
        hex(&self.inner.lock().unwrap().peer_id())
    }

    /// Наш публичный ключ (его передаём собеседникам).
    pub fn public_key(&self) -> Vec<u8> {
        self.inner.lock().unwrap().public_key()
    }

    /// Отправить сообщение в группу. Возвращает готовый конверт (байты)
    /// для передачи в транспорт.
    pub fn post(&self, group_id: Vec<u8>, text: String) -> Result<Vec<u8>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let env = engine.post(group_id, text)?;
        use prost::Message as _;
        Ok(env.encode_to_vec())
    }

    /// Создать группу: возвращает групповой ключ для раздачи участникам.
    pub fn create_group(&self, group_id: Vec<u8>) -> Vec<u8> {
        let mut engine = self.inner.lock().unwrap();
        engine.create_group(group_id).to_vec()
    }

    /// Вступить в группу по ключу (получен по защищённому каналу).
    pub fn join_group(&self, group_id: Vec<u8>, key: Vec<u8>) -> Result<(), FfiError> {
        let key: [u8; 32] = key
            .as_slice()
            .try_into()
            .map_err(|_| FfiError::Core { reason: "key must be 32 bytes".into() })?;
        let mut engine = self.inner.lock().unwrap();
        engine.join_group(group_id, key);
        Ok(())
    }

    /// Обработать входящий конверт. Возвращает конверты для пересылки.
    pub fn receive(&self, envelope: Vec<u8>) -> Result<Vec<Vec<u8>>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let outcome = engine.handle(&envelope)?;
        use prost::Message as _;
        Ok(outcome.outgoing.iter().map(|e| e.encode_to_vec()).collect())
    }

    /// История группы в причинном порядке.
    pub fn history(&self, group_id: Vec<u8>) -> Vec<FfiMessage> {
        let engine = self.inner.lock().unwrap();
        engine
            .log()
            .ordered(&group_id)
            .into_iter()
            .map(|m| FfiMessage {
                id: m.id,
                group_id: m.group_id,
                author: m.author,
                text: m.text,
                lamport: m.lamport,
                wall_time: m.wall_time,
            })
            .collect()
    }

    /// Известные пиры.
    pub fn peers(&self) -> Vec<FfiPeer> {
        let engine = self.inner.lock().unwrap();
        engine
            .peers()
            .all()
            .map(|p| FfiPeer {
                peer_id: p.peer_id.clone(),
                last_seen: p.last_seen,
                is_relay: p.is_relay,
            })
            .collect()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[uniffi::export]
pub fn default_group_id() -> GroupId {
    b"general".to_vec()
}
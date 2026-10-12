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

/// Участник группы.
#[derive(uniffi::Record)]
pub struct FfiMember {
    pub peer_id: Vec<u8>,
    pub public_key: Vec<u8>,
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
        let store = if db_path == ":memory:" {
            p2pchat_storage::Store::open_in_memory()
        } else {
            p2pchat_storage::Store::open(&db_path)
        }
        .map_err(|e| FfiError::Core { reason: e.to_string() })?;

        // Личность: загружаем сохранённую или создаём и сохраняем.
        let identity = match store
            .get_meta("identity")
            .map_err(|e| FfiError::Core { reason: e.to_string() })?
        {
            Some(bytes) => p2pchat_crypto::Identity::from_secret_bytes(&bytes)
                .map_err(|e| FfiError::Core { reason: e.to_string() })?,
            None => {
                let id = p2pchat_crypto::Identity::generate();
                store
                    .set_meta("identity", &id.secret_bytes())
                    .map_err(|e| FfiError::Core { reason: e.to_string() })?;
                id
            }
        };

        let log = Log::with_store(store)?;
        let mut engine = GossipEngine::new(identity, log);
        engine.load_state()?;
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

    /// Создать группу и сразу вернуть инвайт-строку (ссылка/QR).
    pub fn create_group_invite(&self, group_id: Vec<u8>) -> String {
        let mut engine = self.inner.lock().unwrap();
        engine.create_group(group_id.clone());
        engine.invite_for(&group_id).unwrap_or_default()
    }

    /// Инвайт существующей группы.
    pub fn group_invite(&self, group_id: Vec<u8>) -> Option<String> {
        self.inner.lock().unwrap().invite_for(&group_id)
    }

    /// Вступить в группу по инвайт-строке. Возвращает id группы.
    pub fn join_from_invite(&self, invite: String) -> Result<Vec<u8>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        Ok(engine.join_from_invite(&invite)?)
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

    /// Список групп, в которых мы состоим.
    pub fn list_groups(&self) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().groups()
    }

    /// Участники группы.
    pub fn members(&self, group_id: Vec<u8>) -> Vec<FfiMember> {
        self.inner
            .lock()
            .unwrap()
            .members(&group_id)
            .into_iter()
            .map(|m| FfiMember { peer_id: m.peer_id, public_key: m.public_key })
            .collect()
    }

    /// Соседи группы (кому пересылаем / с кем соединяемся).
    pub fn neighbors(&self, group_id: Vec<u8>) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().neighbors(&group_id)
    }

    /// Конверт с нашим входом в группу — разослать соседям.
    pub fn join_envelope(&self, group_id: Vec<u8>) -> Result<Option<Vec<u8>>, FfiError> {
        let engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine.join_envelope(&group_id)?.map(|e| e.encode_to_vec()))
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
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

/// Конверт для отправки с адресатами (публичные ключи).
/// Пустой `targets` — разослать всем подключённым.
#[derive(uniffi::Record)]
pub struct FfiOutgoing {
    pub envelope: Vec<u8>,
    pub targets: Vec<Vec<u8>>,
}

/// Результат вступления по инвайту: id группы и публичный ключ приглашающего
/// (к нему подключаемся для bootstrap).
#[derive(uniffi::Record)]
pub struct FfiJoin {
    pub group_id: Vec<u8>,
    pub host_key: Vec<u8>,
}

/// Страница истории: сообщения (по возрастанию) и есть ли ещё более старые.
#[derive(uniffi::Record)]
pub struct FfiHistoryPage {
    pub messages: Vec<FfiMessage>,
    pub has_more: bool,
}

/// Ядро чата, которым владеет Android-слой.
#[derive(uniffi::Object)]
pub struct ChatCore {
    inner: Mutex<GossipEngine>,
}

#[uniffi::export]
impl ChatCore {
    /// Создать ядро. `db_path` — файл SQLite (или ":memory:").
    /// Личность хранится отдельным файлом рядом с БД (`identity.key`),
    /// чтобы эксперименты со схемой БД не сбрасывали аккаунт.
    #[uniffi::constructor]
    pub fn new(db_path: String) -> Result<Arc<Self>, FfiError> {
        let store = if db_path == ":memory:" {
            p2pchat_storage::Store::open_in_memory()
        } else {
            p2pchat_storage::Store::open(&db_path)
        }
        .map_err(|e| FfiError::Core { reason: e.to_string() })?;

        let identity_path = identity_path_for(&db_path);
        let identity = load_or_create_identity(identity_path.as_deref(), &store)?;

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

    /// Отправить сообщение в группу. Возвращает конверт с адресатами.
    pub fn post(&self, group_id: Vec<u8>, text: String) -> Result<FfiOutgoing, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let o = engine.post(group_id, text)?;
        use prost::Message as _;
        Ok(FfiOutgoing { envelope: o.envelope.encode_to_vec(), targets: o.targets })
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

    /// Вступить в группу по инвайт-строке. Возвращает id группы и ключ хозяина.
    pub fn join_from_invite(&self, invite: String) -> Result<FfiJoin, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let (group_id, host_key) = engine.join_from_invite_full(&invite)?;
        Ok(FfiJoin { group_id, host_key })
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

    /// Отметить группу прочитанной (до последнего сообщения).
    pub fn mark_read(&self, group_id: Vec<u8>) -> Result<(), FfiError> {
        self.inner.lock().unwrap().mark_read(&group_id)?;
        Ok(())
    }

    /// Число непрочитанных сообщений в группе.
    pub fn unread_count(&self, group_id: Vec<u8>) -> Result<u64, FfiError> {
        Ok(self.inner.lock().unwrap().unread_count(&group_id)?)
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

    /// Соседи группы (peer_id).
    pub fn neighbors(&self, group_id: Vec<u8>) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().neighbors(&group_id)
    }

    /// Публичные ключи соседей группы (кому соединяться/пересылать).
    pub fn neighbor_keys(&self, group_id: Vec<u8>) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().neighbor_keys(&group_id)
    }

    /// Группы, где мы состоим и есть участник с таким публичным ключом.
    pub fn groups_with(&self, public_key: Vec<u8>) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().groups_with(&public_key)
    }

    /// Merkle-digest группы, адресованный конкретному пиру (офлайн-догон).
    pub fn sync_digest_to(&self, group_id: Vec<u8>, target_key: Vec<u8>) -> Result<FfiOutgoing, FfiError> {
        let engine = self.inner.lock().unwrap();
        let o = engine.sync_digest_to(&group_id, &target_key)?;
        use prost::Message as _;
        Ok(FfiOutgoing { envelope: o.envelope.encode_to_vec(), targets: o.targets })
    }

    /// Конверт с нашим входом в группу — разослать соседям.
    pub fn join_envelope(&self, group_id: Vec<u8>) -> Result<Option<FfiOutgoing>, FfiError> {
        let engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .join_envelope(&group_id)?
            .map(|o| FfiOutgoing { envelope: o.envelope.encode_to_vec(), targets: o.targets }))
    }

    /// Обработать входящий конверт. Возвращает конверты для пересылки (с адресатами).
    pub fn receive(&self, envelope: Vec<u8>) -> Result<Vec<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let outcome = engine.handle(&envelope)?;
        use prost::Message as _;
        Ok(outcome
            .outgoing
            .into_iter()
            .map(|o| FfiOutgoing { envelope: o.envelope.encode_to_vec(), targets: o.targets })
            .collect())
    }

    /// Вся история группы в причинном порядке (для тестов/утилит).
    pub fn history(&self, group_id: Vec<u8>) -> Vec<FfiMessage> {
        let engine = self.inner.lock().unwrap();
        engine.log().ordered(&group_id).into_iter().map(to_ffi_msg).collect()
    }

    /// Окно истории: не более `limit` сообщений до `before_id` (не включая его).
    /// `before_id = None` — последние `limit`. `has_more` — есть ли более старые.
    pub fn history_window(
        &self,
        group_id: Vec<u8>,
        before_id: Option<Vec<u8>>,
        limit: u32,
    ) -> FfiHistoryPage {
        let engine = self.inner.lock().unwrap();
        let all = engine.log().ordered(&group_id);
        let end = match &before_id {
            Some(id) => all.iter().position(|m| &m.id == id).unwrap_or(all.len()),
            None => all.len(),
        };
        let limit = limit as usize;
        let start = end.saturating_sub(limit);
        let has_more = start > 0;
        let messages = all[start..end].iter().cloned().map(to_ffi_msg).collect();
        FfiHistoryPage { messages, has_more }
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

use std::path::{Path, PathBuf};

/// Путь к файлу личности рядом с БД (None для in-memory).
fn identity_path_for(db_path: &str) -> Option<PathBuf> {
    if db_path == ":memory:" {
        return None;
    }
    let dir = Path::new(db_path).parent().filter(|d| !d.as_os_str().is_empty());
    Some(match dir {
        Some(d) => d.join("identity.key"),
        None => PathBuf::from("identity.key"),
    })
}

/// Загрузить личность из отдельного файла; при отсутствии — разово перенести
/// из старой БД (`meta.identity`) или создать новую.
fn load_or_create_identity(
    path: Option<&Path>,
    store: &p2pchat_storage::Store,
) -> Result<p2pchat_crypto::Identity, FfiError> {
    // 1) отдельный файл
    if let Some(p) = path {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(id) = p2pchat_crypto::Identity::from_secret_bytes(&bytes) {
                return Ok(id);
            }
        }
    }
    // 2) миграция из БД (старые установки)
    if let Some(bytes) = store.get_meta("identity").map_err(io_err)? {
        let id = p2pchat_crypto::Identity::from_secret_bytes(&bytes)
            .map_err(|e| FfiError::Core { reason: e.to_string() })?;
        if let Some(p) = path {
            write_atomic(p, &id.secret_bytes()).map_err(io_err)?;
        }
        return Ok(id);
    }
    // 3) новая личность
    let id = p2pchat_crypto::Identity::generate();
    match path {
        Some(p) => write_atomic(p, &id.secret_bytes()).map_err(io_err)?,
        None => store.set_meta("identity", &id.secret_bytes()).map_err(io_err)?,
    }
    Ok(id)
}

/// Атомарная запись файла (temp + rename).
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("key.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn io_err<E: std::fmt::Display>(e: E) -> FfiError {
    FfiError::Core { reason: e.to_string() }
}

fn to_ffi_msg(m: p2pchat_core::proto::ChatMessage) -> FfiMessage {
    FfiMessage {
        id: m.id,
        group_id: m.group_id,
        author: m.author,
        text: m.text,
        lamport: m.lamport,
        wall_time: m.wall_time,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[uniffi::export]
pub fn default_group_id() -> GroupId {
    b"general".to_vec()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_lives_in_separate_file_and_survives_db_reopen() {
        let dir = std::env::temp_dir().join(format!("p2pchat-ffi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("chat.db");
        let db = db.to_str().unwrap();
        let path = identity_path_for(db).unwrap();

        let id1 = {
            let store = p2pchat_storage::Store::open(db).unwrap();
            load_or_create_identity(Some(&path), &store).unwrap()
        };
        assert!(path.exists(), "файл личности создан");

        // переоткрываем БД — личность берётся из файла и совпадает
        let id2 = {
            let store = p2pchat_storage::Store::open(db).unwrap();
            load_or_create_identity(Some(&path), &store).unwrap()
        };
        assert_eq!(id1.secret_bytes(), id2.secret_bytes());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_window_paginates() {
        let core = ChatCore::new(":memory:".into()).unwrap();
        let g = b"g".to_vec();
        core.create_group_invite(g.clone());
        for i in 0..120 {
            core.post(g.clone(), format!("m{i}")).unwrap();
        }

        let page1 = core.history_window(g.clone(), None, 50);
        assert_eq!(page1.messages.len(), 50);
        assert!(page1.has_more);

        let before1 = page1.messages.first().unwrap().id.clone();
        let page2 = core.history_window(g.clone(), Some(before1), 50);
        assert_eq!(page2.messages.len(), 50);
        assert!(page2.has_more);

        let before2 = page2.messages.first().unwrap().id.clone();
        let page3 = core.history_window(g.clone(), Some(before2), 50);
        assert_eq!(page3.messages.len(), 20);
        assert!(!page3.has_more);
    }
}

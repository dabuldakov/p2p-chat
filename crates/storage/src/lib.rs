//! Хранилище узла: append-only лог сообщений + таблица пиров.
//!
//! Специально не зависит от `core` — работает с примитивами, чтобы не было
//! циклических зависимостей. `core` сериализует/десериализует свои типы поверх.

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub id: Vec<u8>,
    pub group_id: Vec<u8>,
    pub author: Vec<u8>,
    pub lamport: u64,
    pub wall_time: u64,
    pub text: String,
    pub parents: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct StoredPeer {
    pub peer_id: Vec<u8>,
    pub last_seen: u64,
    pub is_relay: bool,
    pub hops: u32,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &str) -> Result<Self, StorageError> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, StorageError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS messages (
                id        BLOB PRIMARY KEY,
                group_id  BLOB NOT NULL,
                author    BLOB NOT NULL,
                lamport   INTEGER NOT NULL,
                wall_time INTEGER NOT NULL,
                text      TEXT NOT NULL,
                parents   BLOB NOT NULL,       -- JSON-массив хешей
                received  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_messages_group
                ON messages(group_id, lamport);

            CREATE TABLE IF NOT EXISTS peers (
                peer_id   BLOB PRIMARY KEY,
                endpoints BLOB NOT NULL,       -- JSON
                last_seen INTEGER NOT NULL,
                is_relay  INTEGER NOT NULL,
                hops      INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS group_keys (
                group_id BLOB PRIMARY KEY,
                key      BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS membership (
                group_id   BLOB NOT NULL,
                peer_id    BLOB NOT NULL,
                public_key BLOB NOT NULL,
                joined     INTEGER NOT NULL,
                lamport    INTEGER NOT NULL,
                event_id   BLOB NOT NULL,
                PRIMARY KEY (group_id, peer_id)
            );
            "#,
        )?;
        Ok(())
    }

    /// Идемпотентная вставка: если id уже есть — ничего не меняем.
    pub fn insert_message(&self, m: &StoredMessage) -> Result<bool, StorageError> {
        let parents = serde_json::to_vec(&m.parents).unwrap_or_default();
        let now = unix_now();
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO messages
             (id, group_id, author, lamport, wall_time, text, parents, received)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                m.id, m.group_id, m.author, m.lamport as i64,
                m.wall_time as i64, m.text, parents, now as i64
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn has_message(&self, id: &[u8]) -> Result<bool, StorageError> {
        let found: Option<i64> = self
            .conn
            .query_row("SELECT 1 FROM messages WHERE id = ?1", params![id], |r| r.get(0))
            .optional()?;
        Ok(found.is_some())
    }

    /// Сообщения группы в порядке CRDT: (lamport, id).
    pub fn messages_for_group(
        &self,
        group_id: &[u8],
        limit: i64,
    ) -> Result<Vec<StoredMessage>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, group_id, author, lamport, wall_time, text, parents
             FROM messages WHERE group_id = ?1
             ORDER BY lamport ASC, id ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![group_id, limit], |r| {
            let parents: Vec<Vec<u8>> =
                serde_json::from_slice(&r.get::<_, Vec<u8>>(6)?).unwrap_or_default();
            Ok(StoredMessage {
                id: r.get(0)?,
                group_id: r.get(1)?,
                author: r.get(2)?,
                lamport: r.get::<_, i64>(3)? as u64,
                wall_time: r.get::<_, i64>(4)? as u64,
                text: r.get(5)?,
                parents,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Все сообщения (для загрузки лога при старте).
    pub fn all_messages(&self) -> Result<Vec<StoredMessage>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, group_id, author, lamport, wall_time, text, parents
             FROM messages ORDER BY lamport ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            let parents: Vec<Vec<u8>> =
                serde_json::from_slice(&r.get::<_, Vec<u8>>(6)?).unwrap_or_default();
            Ok(StoredMessage {
                id: r.get(0)?,
                group_id: r.get(1)?,
                author: r.get(2)?,
                lamport: r.get::<_, i64>(3)? as u64,
                wall_time: r.get::<_, i64>(4)? as u64,
                text: r.get(5)?,
                parents,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn upsert_peer(&self, p: &StoredPeer) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO peers (peer_id, endpoints, last_seen, is_relay, hops)
             VALUES (?1, '[]', ?2, ?3, ?4)
             ON CONFLICT(peer_id) DO UPDATE SET
                last_seen = excluded.last_seen,
                is_relay  = excluded.is_relay,
                hops      = excluded.hops",
            params![p.peer_id, p.last_seen as i64, p.is_relay as i64, p.hops as i64],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_meta(&self, key: &str, value: &[u8]) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn set_group_key(&self, group_id: &[u8], key: &[u8]) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO group_keys (group_id, key) VALUES (?1, ?2)
             ON CONFLICT(group_id) DO UPDATE SET key = excluded.key",
            params![group_id, key],
        )?;
        Ok(())
    }

    pub fn all_group_keys(&self) -> Result<Vec<(Vec<u8>, Vec<u8>)>, StorageError> {
        let mut stmt = self.conn.prepare("SELECT group_id, key FROM group_keys")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_membership(
        &self,
        group_id: &[u8],
        peer_id: &[u8],
        public_key: &[u8],
        joined: bool,
        lamport: u64,
        event_id: &[u8],
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO membership (group_id, peer_id, public_key, joined, lamport, event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(group_id, peer_id) DO UPDATE SET
                public_key = excluded.public_key,
                joined     = excluded.joined,
                lamport    = excluded.lamport,
                event_id   = excluded.event_id",
            params![group_id, peer_id, public_key, joined as i64, lamport as i64, event_id],
        )?;
        Ok(())
    }

    /// Все строки состава: (group_id, peer_id, public_key, joined, lamport, event_id).
    pub fn all_membership(
        &self,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>, Vec<u8>, bool, u64, Vec<u8>)>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT group_id, peer_id, public_key, joined, lamport, event_id FROM membership",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, i64>(3)? != 0,
                r.get::<_, i64>(4)? as u64,
                r.get(5)?,
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
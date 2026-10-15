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

/// Строка ключа группы: (group_id, key).
pub type GroupKeyRow = (Vec<u8>, Vec<u8>);
/// Строка состава: (group_id, peer_id, public_key, joined, lamport, event_id).
pub type MembershipRow = (Vec<u8>, Vec<u8>, Vec<u8>, bool, u64, Vec<u8>);

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
        // PRAGMA: WAL + разумные значения для надёжности и конкурентности.
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA busy_timeout = 5000;
            PRAGMA foreign_keys = ON;
            PRAGMA temp_store = MEMORY;
            "#,
        )?;

        // Версия схемы: раньше её не было (0). Базовая схема — v1.
        let version: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?;

        if version < 1 {
            self.conn.execute_batch(
                r#"
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
                CREATE INDEX IF NOT EXISTS idx_messages_lamport
                    ON messages(lamport);

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
            self.conn.execute_batch("PRAGMA user_version = 1;")?;
        }

        if version < 2 {
            self.migrate_v2()?;
            self.conn.execute_batch("PRAGMA user_version = 2;")?;
        }

        Ok(())
    }

    /// v2: родители сообщений — в отдельной таблице (вместо JSON в messages).
    fn migrate_v2(&self) -> Result<(), StorageError> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS message_parents (
                message_id BLOB NOT NULL,
                parent_id  BLOB NOT NULL,
                PRIMARY KEY (message_id, parent_id)
            );
            CREATE INDEX IF NOT EXISTS idx_parents_parent ON message_parents(parent_id);
            "#,
        )?;

        // Если в старой таблице ещё есть столбец parents — переносим и пересоздаём.
        if self.has_column("messages", "parents")? {
            let rows: Vec<(Vec<u8>, Vec<u8>)> = {
                let mut stmt = self.conn.prepare("SELECT id, parents FROM messages")?;
                let it = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                it.collect::<Result<_, _>>()?
            };

            let tx = self.conn.unchecked_transaction()?;
            {
                let mut ins = tx.prepare(
                    "INSERT OR IGNORE INTO message_parents (message_id, parent_id) VALUES (?1, ?2)",
                )?;
                for (id, parents_json) in &rows {
                    let parents: Vec<Vec<u8>> =
                        serde_json::from_slice(parents_json).unwrap_or_default();
                    for p in parents {
                        ins.execute(params![id, p])?;
                    }
                }
            }
            tx.execute_batch(
                r#"
                CREATE TABLE messages_new (
                    id        BLOB PRIMARY KEY,
                    group_id  BLOB NOT NULL,
                    author    BLOB NOT NULL,
                    lamport   INTEGER NOT NULL,
                    wall_time INTEGER NOT NULL,
                    text      TEXT NOT NULL,
                    received  INTEGER NOT NULL
                );
                INSERT INTO messages_new (id, group_id, author, lamport, wall_time, text, received)
                    SELECT id, group_id, author, lamport, wall_time, text, received FROM messages;
                DROP TABLE messages;
                ALTER TABLE messages_new RENAME TO messages;
                CREATE INDEX IF NOT EXISTS idx_messages_group ON messages(group_id, lamport);
                CREATE INDEX IF NOT EXISTS idx_messages_lamport ON messages(lamport);
                "#,
            )?;
            tx.commit()?;
        }

        Ok(())
    }

    fn has_column(&self, table: &str, column: &str) -> Result<bool, StorageError> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Идемпотентная вставка сообщения и обновление часов — одной транзакцией.
    pub fn insert_message(&self, m: &StoredMessage, lamport: u64) -> Result<bool, StorageError> {
        let now = unix_now();
        let tx = self.conn.unchecked_transaction()?;
        let changed = tx.execute(
            "INSERT OR IGNORE INTO messages
             (id, group_id, author, lamport, wall_time, text, received)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                m.id, m.group_id, m.author, m.lamport as i64,
                m.wall_time as i64, m.text, now as i64
            ],
        )?;
        if changed > 0 {
            let mut ins = tx.prepare(
                "INSERT OR IGNORE INTO message_parents (message_id, parent_id) VALUES (?1, ?2)",
            )?;
            for p in &m.parents {
                ins.execute(params![m.id, p])?;
            }
        }
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('lamport', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![lamport.to_le_bytes().as_slice()],
        )?;
        tx.commit()?;
        Ok(changed > 0)
    }

    /// Пакетная вставка (например, ответ синка) — одной транзакцией.
    pub fn insert_messages(
        &self,
        messages: &[StoredMessage],
        lamport: u64,
    ) -> Result<usize, StorageError> {
        let now = unix_now();
        let tx = self.conn.unchecked_transaction()?;
        let mut inserted = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO messages
                 (id, group_id, author, lamport, wall_time, text, received)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
            )?;
            let mut pstmt = tx.prepare(
                "INSERT OR IGNORE INTO message_parents (message_id, parent_id) VALUES (?1, ?2)",
            )?;
            for m in messages {
                let n = stmt.execute(params![
                    m.id, m.group_id, m.author, m.lamport as i64,
                    m.wall_time as i64, m.text, now as i64
                ])?;
                if n > 0 {
                    for p in &m.parents {
                        pstmt.execute(params![m.id, p])?;
                    }
                    inserted += n;
                }
            }
        }
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('lamport', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![lamport.to_le_bytes().as_slice()],
        )?;
        tx.commit()?;
        Ok(inserted)
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
        let parents = self.parents_map()?;
        let mut stmt = self.conn.prepare(
            "SELECT id, group_id, author, lamport, wall_time, text
             FROM messages WHERE group_id = ?1
             ORDER BY lamport ASC, id ASC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![group_id, limit], |r| {
            Ok(StoredMessage {
                id: r.get(0)?,
                group_id: r.get(1)?,
                author: r.get(2)?,
                lamport: r.get::<_, i64>(3)? as u64,
                wall_time: r.get::<_, i64>(4)? as u64,
                text: r.get(5)?,
                parents: Vec::new(),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            let mut m = row?;
            if let Some(ps) = parents.get(&m.id) {
                m.parents = ps.clone();
            }
            out.push(m);
        }
        Ok(out)
    }

    /// Все сообщения (для загрузки лога при старте).
    pub fn all_messages(&self) -> Result<Vec<StoredMessage>, StorageError> {
        let parents = self.parents_map()?;
        let mut stmt = self.conn.prepare(
            "SELECT id, group_id, author, lamport, wall_time, text
             FROM messages ORDER BY lamport ASC, id ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(StoredMessage {
                id: r.get(0)?,
                group_id: r.get(1)?,
                author: r.get(2)?,
                lamport: r.get::<_, i64>(3)? as u64,
                wall_time: r.get::<_, i64>(4)? as u64,
                text: r.get(5)?,
                parents: Vec::new(),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            let mut m = row?;
            if let Some(ps) = parents.get(&m.id) {
                m.parents = ps.clone();
            }
            out.push(m);
        }
        Ok(out)
    }

    /// Карта message_id -> parents.
    fn parents_map(
        &self,
    ) -> Result<std::collections::HashMap<Vec<u8>, Vec<Vec<u8>>>, StorageError> {
        let mut stmt = self.conn.prepare("SELECT message_id, parent_id FROM message_parents")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        let mut map: std::collections::HashMap<Vec<u8>, Vec<Vec<u8>>> = Default::default();
        for row in rows {
            let (id, parent) = row?;
            map.entry(id).or_default().push(parent);
        }
        Ok(map)
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

    pub fn all_group_keys(&self) -> Result<Vec<GroupKeyRow>, StorageError> {
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
    pub fn all_membership(&self) -> Result<Vec<MembershipRow>, StorageError> {
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
#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &[u8], lamport: u64) -> StoredMessage {
        StoredMessage {
            id: id.to_vec(),
            group_id: b"g".to_vec(),
            author: b"a".to_vec(),
            lamport,
            wall_time: 1,
            text: "t".into(),
            parents: vec![],
        }
    }

    #[test]
    fn schema_version_is_set() {
        let store = Store::open_in_memory().unwrap();
        let v: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2);
    }

    #[test]
    fn insert_updates_lamport_in_one_tx() {
        let store = Store::open_in_memory().unwrap();
        assert!(store.insert_message(&msg(b"m1", 5), 5).unwrap());
        // idempotent
        assert!(!store.insert_message(&msg(b"m1", 5), 5).unwrap());
        assert!(store.has_message(b"m1").unwrap());
        let lamport = store.get_meta("lamport").unwrap().unwrap();
        assert_eq!(u64::from_le_bytes(lamport.try_into().unwrap()), 5);
    }

    #[test]
    fn batch_insert_counts() {
        let store = Store::open_in_memory().unwrap();
        let batch: Vec<_> = (0..10).map(|i| msg(format!("m{i}").as_bytes(), i)).collect();
        let n = store.insert_messages(&batch, 9).unwrap();
        assert_eq!(n, 10);
        // повтор — уже ничего нового
        assert_eq!(store.insert_messages(&batch, 9).unwrap(), 0);
    }

    #[test]
    fn migrate_v1_to_v2_moves_parents_without_loss() {
        let mut path = std::env::temp_dir();
        path.push(format!("p2pchat-mig-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let path = path.to_str().unwrap().to_string();

        // создаём «старую» (v1) схему с родителями в JSON
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE messages (
                    id BLOB PRIMARY KEY, group_id BLOB NOT NULL, author BLOB NOT NULL,
                    lamport INTEGER NOT NULL, wall_time INTEGER NOT NULL, text TEXT NOT NULL,
                    parents BLOB NOT NULL, received INTEGER NOT NULL);
                CREATE TABLE peers (peer_id BLOB PRIMARY KEY, endpoints BLOB NOT NULL,
                    last_seen INTEGER NOT NULL, is_relay INTEGER NOT NULL, hops INTEGER NOT NULL);
                CREATE TABLE meta (key TEXT PRIMARY KEY, value BLOB NOT NULL);
                CREATE TABLE group_keys (group_id BLOB PRIMARY KEY, key BLOB NOT NULL);
                CREATE TABLE membership (group_id BLOB NOT NULL, peer_id BLOB NOT NULL,
                    public_key BLOB NOT NULL, joined INTEGER NOT NULL, lamport INTEGER NOT NULL,
                    event_id BLOB NOT NULL, PRIMARY KEY (group_id, peer_id));
                PRAGMA user_version = 1;
                "#,
            )
            .unwrap();
            let parents = serde_json::to_vec(&vec![vec![1u8, 2, 3]]).unwrap();
            conn.execute(
                "INSERT INTO messages VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![b"m1".to_vec(), b"g".to_vec(), b"a".to_vec(), 1i64, 1i64, "hi", parents, 1i64],
            )
            .unwrap();
        }

        // открываем через Store — должна пройти миграция v1 -> v2
        let store = Store::open(&path).unwrap();
        let msgs = store.all_messages().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].parents, vec![vec![1u8, 2, 3]], "родители перенесены");
        assert!(!store.has_column("messages", "parents").unwrap(), "старый столбец убран");
        let v: i64 = store.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 2);

        let _ = std::fs::remove_file(&path);
    }
}

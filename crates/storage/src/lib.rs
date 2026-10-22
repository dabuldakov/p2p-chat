//! Хранилище узла: append-only лог сообщений + таблица пиров.
//!
//! Специально не зависит от `core` — работает с примитивами, чтобы не было
//! циклических зависимостей. `core` сериализует/десериализует свои типы поверх.

use rusqlite::{params, Connection, OptionalExtension};

/// Строка профиля: (peer_id, nickname, lamport, avatar).
pub type ProfileRow = (Vec<u8>, String, u64, Option<Vec<u8>>);
/// Строка правки: (message_id, text, lamport, wall_time).
pub type EditRow = (Vec<u8>, String, u64, u64);
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
    pub attachment: Option<StoredAttachment>,
}

/// Вложение сообщения. Для гифок храним только ссылку-идентификатор
/// (`kind` + `id`); байты медиа не персистим.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAttachment {
    pub kind: String,
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub url: String,
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

        if version < 3 {
            self.conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS profiles (
                    peer_id  BLOB PRIMARY KEY,
                    nickname TEXT NOT NULL,
                    lamport  INTEGER NOT NULL
                );
                "#,
            )?;
            self.conn.execute_batch("PRAGMA user_version = 3;")?;
        }

        if version < 4 {
            self.migrate_v4()?;
            self.conn.execute_batch("PRAGMA user_version = 4;")?;
        }

        if version < 5 {
            if !self.has_column("profiles", "avatar")? {
                self.conn.execute_batch("ALTER TABLE profiles ADD COLUMN avatar BLOB;")?;
            }
            self.conn.execute_batch("PRAGMA user_version = 5;")?;
        }

        if version < 6 {
            self.conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS message_edits (
                    message_id BLOB PRIMARY KEY,
                    text       TEXT NOT NULL,
                    lamport    INTEGER NOT NULL,
                    wall_time  INTEGER NOT NULL
                );
                "#,
            )?;
            self.conn.execute_batch("PRAGMA user_version = 6;")?;
        }

        if version < 7 {
            self.conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS message_deletes (
                    message_id BLOB PRIMARY KEY
                );
                "#,
            )?;
            self.conn.execute_batch("PRAGMA user_version = 7;")?;
        }

        Ok(())
    }

    /// v4: вложение сообщения — колонками на строке (одно вложение на сообщение).
    fn migrate_v4(&self) -> Result<(), StorageError> {
        for (col, ty) in [
            ("attachment_kind", "TEXT"),
            ("attachment_id", "TEXT"),
            ("attachment_width", "INTEGER"),
            ("attachment_height", "INTEGER"),
            ("attachment_url", "TEXT"),
        ] {
            if !self.has_column("messages", col)? {
                self.conn
                    .execute_batch(&format!("ALTER TABLE messages ADD COLUMN {col} {ty};"))?;
            }
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
        let (ak, ai, aw, ah, au) = attachment_cols(m);
        let changed = tx.execute(
            "INSERT OR IGNORE INTO messages
             (id, group_id, author, lamport, wall_time, text, received,
              attachment_kind, attachment_id, attachment_width, attachment_height, attachment_url)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                m.id, m.group_id, m.author, m.lamport as i64,
                m.wall_time as i64, m.text, now as i64,
                ak, ai, aw, ah, au
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
                 (id, group_id, author, lamport, wall_time, text, received,
                  attachment_kind, attachment_id, attachment_width, attachment_height, attachment_url)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            )?;
            let mut pstmt = tx.prepare(
                "INSERT OR IGNORE INTO message_parents (message_id, parent_id) VALUES (?1, ?2)",
            )?;
            for m in messages {
                let (ak, ai, aw, ah, au) = attachment_cols(m);
                let n = stmt.execute(params![
                    m.id, m.group_id, m.author, m.lamport as i64,
                    m.wall_time as i64, m.text, now as i64,
                    ak, ai, aw, ah, au
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
            "SELECT id, group_id, author, lamport, wall_time, text,
                    attachment_kind, attachment_id, attachment_width, attachment_height, attachment_url
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
                attachment: read_attachment(r)?,
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
            "SELECT id, group_id, author, lamport, wall_time, text,
                    attachment_kind, attachment_id, attachment_width, attachment_height, attachment_url
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
                attachment: read_attachment(r)?,
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

    /// Отметка «прочитано до» для группы (id последнего прочитанного сообщения).
    pub fn set_read(&self, group_id: &[u8], last_id: &[u8]) -> Result<(), StorageError> {
        self.set_meta(&format!("read:{}", hex(group_id)), last_id)
    }

    pub fn get_read(&self, group_id: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        self.get_meta(&format!("read:{}", hex(group_id)))
    }

    pub fn set_profile(
        &self,
        peer_id: &[u8],
        nickname: &str,
        lamport: u64,
        avatar: Option<&[u8]>,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO profiles (peer_id, nickname, lamport, avatar) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(peer_id) DO UPDATE SET
                nickname = excluded.nickname,
                lamport = excluded.lamport,
                avatar = excluded.avatar",
            params![peer_id, nickname, lamport as i64, avatar],
        )?;
        Ok(())
    }

    /// (peer_id, nickname, lamport, avatar)
    pub fn all_profiles(&self) -> Result<Vec<ProfileRow>, StorageError> {
        let mut stmt =
            self.conn.prepare("SELECT peer_id, nickname, lamport, avatar FROM profiles")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, Option<Vec<u8>>>(3)?,
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Сохранить правку сообщения (LWW по lamport).
    pub fn set_edit(
        &self,
        message_id: &[u8],
        text: &str,
        lamport: u64,
        wall_time: u64,
    ) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT INTO message_edits (message_id, text, lamport, wall_time) VALUES (?1,?2,?3,?4)
             ON CONFLICT(message_id) DO UPDATE SET
                text = excluded.text, lamport = excluded.lamport, wall_time = excluded.wall_time
             WHERE excluded.lamport >= message_edits.lamport",
            params![message_id, text, lamport as i64, wall_time as i64],
        )?;
        Ok(())
    }

    /// Все правки: (message_id, text, lamport, wall_time).
    pub fn all_edits(&self) -> Result<Vec<EditRow>, StorageError> {
        let mut stmt =
            self.conn.prepare("SELECT message_id, text, lamport, wall_time FROM message_edits")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get::<_, i64>(2)? as u64,
                r.get::<_, i64>(3)? as u64,
            ))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Пометить сообщение удалённым (tombstone; необратимо).
    pub fn set_deleted(&self, message_id: &[u8]) -> Result<(), StorageError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO message_deletes (message_id) VALUES (?1)",
            params![message_id],
        )?;
        Ok(())
    }

    /// Все удалённые сообщения.
    pub fn all_deletes(&self) -> Result<Vec<Vec<u8>>, StorageError> {
        let mut stmt = self.conn.prepare("SELECT message_id FROM message_deletes")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
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

    /// Уникальные вложения-гифки группы, свежие сверху (дедуп по kind+id).
    pub fn gifs_for_group(
        &self,
        group_id: &[u8],
        limit: i64,
    ) -> Result<Vec<StoredAttachment>, StorageError> {
        self.gifs_query("group_id = ?1", params![group_id, limit])
    }

    /// Уникальные вложения-гифки, отправленные конкретным автором (свежие сверху).
    pub fn gifs_by_author(
        &self,
        author: &[u8],
        limit: i64,
    ) -> Result<Vec<StoredAttachment>, StorageError> {
        self.gifs_query("author = ?1", params![author, limit])
    }

    fn gifs_query(
        &self,
        filter: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<StoredAttachment>, StorageError> {
        let sql = format!(
            "SELECT attachment_kind, attachment_id, attachment_width, attachment_height, attachment_url,
                    MAX(wall_time) AS wt
             FROM messages
             WHERE {filter} AND attachment_kind IS NOT NULL
             GROUP BY attachment_kind, attachment_id
             ORDER BY wt DESC, attachment_id DESC
             LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params, |r| {
            Ok(StoredAttachment {
                kind: r.get(0)?,
                id: r.get(1)?,
                width: r.get::<_, Option<i64>>(2)?.unwrap_or(0) as u32,
                height: r.get::<_, Option<i64>>(3)?.unwrap_or(0) as u32,
                url: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// Колонки вложения для INSERT (None, если вложения нет).
type AttachmentCols<'a> = (
    Option<&'a str>,
    Option<&'a str>,
    Option<i64>,
    Option<i64>,
    Option<&'a str>,
);

fn attachment_cols(m: &StoredMessage) -> AttachmentCols<'_> {
    match &m.attachment {
        Some(a) => (
            Some(a.kind.as_str()),
            Some(a.id.as_str()),
            Some(a.width as i64),
            Some(a.height as i64),
            Some(a.url.as_str()),
        ),
        None => (None, None, None, None, None),
    }
}

/// Прочитать вложение из строки выборки (колонки 6..10).
fn read_attachment(r: &rusqlite::Row<'_>) -> rusqlite::Result<Option<StoredAttachment>> {
    let kind: Option<String> = r.get(6)?;
    Ok(match kind {
        Some(kind) => Some(StoredAttachment {
            kind,
            id: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
            width: r.get::<_, Option<i64>>(8)?.unwrap_or(0) as u32,
            height: r.get::<_, Option<i64>>(9)?.unwrap_or(0) as u32,
            url: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
        }),
        None => None,
    })
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
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
            attachment: None,
        }
    }

    #[test]
    fn schema_version_is_set() {
        let store = Store::open_in_memory().unwrap();
        let v: i64 = store
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 7);
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
        assert_eq!(v, 7);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn attachment_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let mut m = msg(b"m1", 5);
        m.text = String::new();
        m.attachment = Some(StoredAttachment {
            kind: "klipy".into(),
            id: "abc123".into(),
            width: 220,
            height: 230,
            url: "https://static.klipy.com/x.mp4".into(),
        });
        assert!(store.insert_message(&m, 5).unwrap());

        let back = store.messages_for_group(b"g", 10).unwrap();
        assert_eq!(back.len(), 1);
        let a = back[0].attachment.as_ref().expect("вложение сохранено");
        assert_eq!(a.kind, "klipy");
        assert_eq!(a.id, "abc123");
        assert_eq!((a.width, a.height), (220, 230));
        assert_eq!(a.url, "https://static.klipy.com/x.mp4");

        // у обычного сообщения вложения нет
        assert!(store.insert_message(&msg(b"m2", 6), 6).unwrap());
        let back = store.messages_for_group(b"g", 10).unwrap();
        let plain = back.iter().find(|m| m.id == b"m2".to_vec()).unwrap();
        assert!(plain.attachment.is_none());
    }

    #[test]
    fn gifs_queries_dedup_and_order() {
        let store = Store::open_in_memory().unwrap();
        let mut a = msg(b"a1", 1);
        a.attachment = Some(StoredAttachment { kind: "klipy".into(), id: "g1".into(), width: 1, height: 1, url: String::new() });
        a.wall_time = 100;
        let mut a2 = msg(b"a2", 2);
        a2.group_id = b"g2".to_vec();
        a2.author = b"b".to_vec();
        a2.attachment = Some(StoredAttachment { kind: "klipy".into(), id: "g2".into(), width: 1, height: 1, url: String::new() });
        a2.wall_time = 200;
        // дубликат g1 тем же автором позже — не должен дублироваться
        let mut a1dup = msg(b"a1dup", 3);
        a1dup.attachment = Some(StoredAttachment { kind: "klipy".into(), id: "g1".into(), width: 1, height: 1, url: String::new() });
        a1dup.wall_time = 300;

        store.insert_message(&a, 1).unwrap();
        store.insert_message(&a2, 2).unwrap();
        store.insert_message(&a1dup, 3).unwrap();

        let group_g = store.gifs_for_group(b"g", 50).unwrap();
        assert_eq!(group_g.len(), 1, "дедуп по kind+id");
        assert_eq!(group_g[0].id, "g1");

        let by_a = store.gifs_by_author(b"a", 50).unwrap();
        assert_eq!(by_a.len(), 1);
        assert_eq!(by_a[0].id, "g1");

        let by_b = store.gifs_by_author(b"b", 50).unwrap();
        assert_eq!(by_b.len(), 1);
        assert_eq!(by_b[0].id, "g2");
    }
    #[test]
    fn meta_and_read_markers() {
        let st = Store::open_in_memory().unwrap();
        assert!(st.get_meta("k").unwrap().is_none());
        st.set_meta("k", b"v1").unwrap();
        st.set_meta("k", b"v2").unwrap();
        assert_eq!(st.get_meta("k").unwrap().unwrap(), b"v2");

        st.set_read(b"g", b"msg").unwrap();
        assert_eq!(st.get_read(b"g").unwrap().unwrap(), b"msg");
        assert!(st.get_read(b"other").unwrap().is_none());
    }

    #[test]
    fn profiles_upsert_with_avatar() {
        let st = Store::open_in_memory().unwrap();
        st.set_profile(b"p", "Ann", 1, None).unwrap();
        let rows = st.all_profiles().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "Ann");
        assert!(rows[0].3.is_none());

        st.set_profile(b"p", "Ann2", 2, Some(&[1, 2, 3])).unwrap();
        let rows = st.all_profiles().unwrap();
        assert_eq!(rows.len(), 1, "upsert по peer_id");
        assert_eq!(rows[0].1, "Ann2");
        assert_eq!(rows[0].3.as_deref(), Some(&[1u8, 2, 3][..]));
    }

    #[test]
    fn edits_are_lww() {
        let st = Store::open_in_memory().unwrap();
        st.set_edit(b"m", "one", 1, 1).unwrap();
        st.set_edit(b"m", "three", 3, 3).unwrap();
        st.set_edit(b"m", "two", 2, 2).unwrap(); // старее — игнорируется
        let rows = st.all_edits().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "three");
        assert_eq!(rows[0].2, 3);
    }

    #[test]
    fn deletes_are_idempotent() {
        let st = Store::open_in_memory().unwrap();
        st.set_deleted(b"m1").unwrap();
        st.set_deleted(b"m1").unwrap();
        st.set_deleted(b"m2").unwrap();
        let mut ids = st.all_deletes().unwrap();
        ids.sort();
        assert_eq!(ids, vec![b"m1".to_vec(), b"m2".to_vec()]);
    }

    #[test]
    fn group_keys_roundtrip() {
        let st = Store::open_in_memory().unwrap();
        st.set_group_key(b"g", b"k1").unwrap();
        st.set_group_key(b"g", b"k2").unwrap();
        let rows = st.all_group_keys().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, b"k2");
    }

    #[test]
    fn membership_upsert() {
        let st = Store::open_in_memory().unwrap();
        st.upsert_membership(b"g", b"p", b"pk", true, 1, b"e1").unwrap();
        st.upsert_membership(b"g", b"p", b"pk2", false, 2, b"e2").unwrap();
        let rows = st.all_membership().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].2, b"pk2");
        assert!(!rows[0].3);
        assert_eq!(rows[0].4, 2);
    }

    #[test]
    fn messages_queries_and_attachments() {
        let st = Store::open_in_memory().unwrap();
        let mut g1 = msg(b"g1m1", 2);
        g1.attachment = Some(StoredAttachment {
            kind: "gif".into(),
            id: "abc".into(),
            width: 10,
            height: 20,
            url: "http://x".into(),
        });
        let mut g2 = msg(b"g2m1", 1);
        g2.group_id = b"g2".to_vec();

        st.insert_message(&g1, 2).unwrap();
        st.insert_message(&g2, 2).unwrap();

        let only_g1 = st.messages_for_group(b"g", 10).unwrap();
        assert_eq!(only_g1.len(), 1);
        assert_eq!(only_g1[0].attachment.as_ref().unwrap().id, "abc");
        assert_eq!(only_g1[0].attachment.as_ref().unwrap().height, 20);

        let all = st.all_messages().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, b"g2m1", "сортировка по lamport");
    }

    #[test]
    fn peers_upsert() {
        let st = Store::open_in_memory().unwrap();
        st.upsert_peer(&StoredPeer { peer_id: b"p".to_vec(), last_seen: 1, is_relay: false, hops: 1 })
            .unwrap();
        st.upsert_peer(&StoredPeer { peer_id: b"p".to_vec(), last_seen: 2, is_relay: true, hops: 0 })
            .unwrap();
    }
}

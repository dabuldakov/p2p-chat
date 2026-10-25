//! Append-only DAG-лог — источник правды узла.
//!
//! Каждое сообщение ссылается на `parents` (текущие головы группы), образуя
//! направленный ациклический граф. Слияние идемпотентно и коммутативно, а
//! порядок восстанавливается топологической сортировкой с тайбрейком по
//! `(lamport, id)`. Это и есть CRDT: у всех реплик рано или поздно
//! одинаковый результат, независимо от порядка доставки.

use std::collections::{HashMap, HashSet};

use p2pchat_storage::{ReactionRow, Store, StoredMessage};

use crate::clock::Lamport;
use crate::proto::ChatMessage;
use crate::{message_id, unix_now, GroupId, MessageId, CoreError};

pub struct Log {
    clock: Lamport,
    /// id -> сообщение
    messages: HashMap<MessageId, ChatMessage>,
    /// группа -> головы (сообщения без известных потомков)
    heads: HashMap<GroupId, HashSet<MessageId>>,
    /// id -> есть ли у него известный потомок
    has_child: HashSet<MessageId>,
    store: Option<Store>,
    /// Мета в памяти (когда store нет — напр. в тестах).
    mem_meta: std::sync::Mutex<HashMap<String, Vec<u8>>>,
}

/// Строка профиля: peer_id, ник, lamport, аватар.
pub type ProfileRow = (Vec<u8>, String, u64, Option<Vec<u8>>);

impl Log {
    pub fn new() -> Self {
        Self {
            clock: Lamport::new(),
            messages: HashMap::new(),
            heads: HashMap::new(),
            has_child: HashSet::new(),
            store: None,
            mem_meta: Default::default(),
        }
    }

    /// Лог с персистентностью.
    pub fn with_store(store: Store) -> Result<Self, CoreError> {
        let mut log = Self::new();
        // восстановление логических часов
        let watermark = store
            .get_meta("lamport")?
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
            .unwrap_or(0);
        log.clock.advance_to(watermark);
        // восстановление сообщений
        let stored = store.all_messages()?;
        log.store = Some(store);
        for m in stored {
            log.insert(ChatMessage {
                id: m.id,
                group_id: m.group_id,
                author: m.author,
                lamport: m.lamport,
                wall_time: m.wall_time,
                text: m.text,
                parents: m.parents,
                attachment: m.attachment.map(|a| crate::proto::Attachment {
                    kind: a.kind,
                    id: a.id,
                    width: a.width,
                    height: a.height,
                    url: a.url,
                    mime: a.mime,
                    size: a.size,
                    duration_ms: a.duration_ms,
                    file_name: a.file_name,
                    thumbnail: a.thumbnail,
                }),
            });
        }
        Ok(log)
    }

    pub fn lamport(&self) -> u64 {
        self.clock.value()
    }

    /// Создать собственное сообщение и добавить его в лог.
    pub fn append_local(
        &mut self,
        group_id: GroupId,
        author: crate::PeerId,
        text: String,
        attachment: Option<crate::proto::Attachment>,
    ) -> Result<ChatMessage, CoreError> {
        let parents: Vec<MessageId> = self
            .heads
            .get(&group_id)
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default();

        let lamport = self.clock.tick();
        let mut msg = ChatMessage {
            id: Vec::new(),
            group_id,
            author,
            lamport,
            wall_time: unix_now(),
            text,
            parents,
            attachment,
        };
        msg.id = message_id(&msg);
        self.insert(msg.clone());
        self.persist(&msg)?;
        Ok(msg)
    }

    /// Слить удалённое сообщение. `Ok(true)` — было новым.
    pub fn merge(&mut self, msg: ChatMessage) -> Result<bool, CoreError> {
        if self.messages.contains_key(&msg.id) {
            return Ok(false);
        }
        self.clock.observe(msg.lamport);
        self.insert(msg.clone());
        self.persist(&msg)?;
        Ok(true)
    }

    fn insert(&mut self, msg: ChatMessage) {
        let id = msg.id.clone();
        let group = msg.group_id.clone();

        for p in &msg.parents {
            self.has_child.insert(p.clone());
        }

        let heads = self.heads.entry(group).or_default();
        for p in &msg.parents {
            heads.remove(p);
        }
        if !self.has_child.contains(&id) {
            heads.insert(id.clone());
        }
        self.messages.insert(id, msg);
    }

    fn persist(&self, msg: &ChatMessage) -> Result<(), CoreError> {
        if let Some(store) = &self.store {
            store.insert_message(
                &StoredMessage {
                    id: msg.id.clone(),
                    group_id: msg.group_id.clone(),
                    author: msg.author.clone(),
                    lamport: msg.lamport,
                    wall_time: msg.wall_time,
                    text: msg.text.clone(),
                    parents: msg.parents.clone(),
                    attachment: msg.attachment.as_ref().map(|a| p2pchat_storage::StoredAttachment {
                        kind: a.kind.clone(),
                        id: a.id.clone(),
                        width: a.width,
                        height: a.height,
                        url: a.url.clone(),
                        mime: a.mime.clone(),
                        size: a.size,
                        duration_ms: a.duration_ms,
                        file_name: a.file_name.clone(),
                        thumbnail: a.thumbnail.clone(),
                    }),
                },
                self.clock.value(),
            )?;
        }
        Ok(())
    }

    pub fn heads(&self, group_id: &[u8]) -> Vec<MessageId> {
        self.heads
            .get(group_id)
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get(&self, id: &[u8]) -> Option<&ChatMessage> {
        self.messages.get(id)
    }

    /// Увеличить логические часы (для событий вне чата, напр. членства).
    pub fn tick(&mut self) -> u64 {
        self.clock.tick()
    }

    /// Сохранить ключ группы.
    pub fn store_group_key(&self, group: &[u8], key: &[u8]) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.set_group_key(group, key)?;
            s.set_meta("lamport", &self.clock.value().to_le_bytes())?;
        }
        Ok(())
    }

    pub fn set_meta(&self, key: &str, value: &[u8]) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.set_meta(key, value)?;
        } else {
            self.mem_meta.lock().unwrap().insert(key.to_string(), value.to_vec());
        }
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<Vec<u8>>, CoreError> {
        Ok(match &self.store {
            Some(s) => s.get_meta(key)?,
            None => self.mem_meta.lock().unwrap().get(key).cloned(),
        })
    }

    /// Отметка «прочитано до» (id последнего прочитанного сообщения группы).
    /// Хранится в meta, поэтому работает и без Store (в памяти, для тестов).
    pub fn set_read(&self, group: &[u8], id: &[u8]) -> Result<(), CoreError> {
        self.set_meta(&format!("read:{}", hex(group)), id)
    }

    pub fn read_marker(&self, group: &[u8]) -> Result<Option<Vec<u8>>, CoreError> {
        self.get_meta(&format!("read:{}", hex(group)))
    }

    pub fn store_profile(
        &self,
        peer_id: &[u8],
        nickname: &str,
        lamport: u64,
        avatar: Option<&[u8]>,
    ) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.set_profile(peer_id, nickname, lamport, avatar)?;
        }
        Ok(())
    }

    /// (peer_id, nickname, lamport, avatar)
    pub fn load_profiles(&self) -> Result<Vec<ProfileRow>, CoreError> {
        Ok(match &self.store {
            Some(s) => s.all_profiles()?,
            None => Vec::new(),
        })
    }

    pub fn store_edit(
        &self,
        message_id: &[u8],
        text: &str,
        lamport: u64,
        wall_time: u64,
    ) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.set_edit(message_id, text, lamport, wall_time)?;
        }
        Ok(())
    }

    /// (message_id, text, lamport, wall_time)
    pub fn load_edits(&self) -> Result<Vec<p2pchat_storage::EditRow>, CoreError> {
        Ok(match &self.store {
            Some(s) => s.all_edits()?,
            None => Vec::new(),
        })
    }

    pub fn store_delete(&self, message_id: &[u8]) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.set_deleted(message_id)?;
        }
        Ok(())
    }

    pub fn load_deletes(&self) -> Result<Vec<Vec<u8>>, CoreError> {
        Ok(match &self.store {
            Some(s) => s.all_deletes()?,
            None => Vec::new(),
        })
    }

    pub fn store_reaction(
        &self,
        target_id: &[u8],
        peer_id: &[u8],
        emoji: &str,
        lamport: u64,
    ) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.set_reaction(target_id, peer_id, emoji, lamport)?;
            s.set_meta("lamport", &self.clock.value().to_le_bytes())?;
        }
        Ok(())
    }

    pub fn remove_reaction(
        &self,
        target_id: &[u8],
        peer_id: &[u8],
        emoji: &str,
    ) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.remove_reaction(target_id, peer_id, emoji)?;
        }
        Ok(())
    }

    pub fn load_reactions(&self) -> Result<Vec<ReactionRow>, CoreError> {
        Ok(match &self.store {
            Some(s) => s.all_reactions()?,
            None => Vec::new(),
        })
    }

    pub fn load_group_keys(&self) -> Result<Vec<p2pchat_storage::GroupKeyRow>, CoreError> {
        Ok(match &self.store {
            Some(s) => s.all_group_keys()?,
            None => Vec::new(),
        })
    }

    /// Сохранить (разрешённое) событие членства.
    pub fn store_membership(&self, ev: &crate::proto::MembershipEvent) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.upsert_membership(&ev.group_id, &ev.peer_id, &ev.public_key, ev.join, ev.lamport, &ev.id)?;
        }
        Ok(())
    }

    pub fn load_membership(&self) -> Result<Vec<crate::proto::MembershipEvent>, CoreError> {
        let rows = match &self.store {
            Some(s) => s.all_membership()?,
            None => return Ok(Vec::new()),
        };
        Ok(rows
            .into_iter()
            .map(|(group_id, peer_id, public_key, join, lamport, id)| crate::proto::MembershipEvent {
                group_id,
                peer_id,
                public_key,
                join,
                lamport,
                author: Vec::new(),
                id,
            })
            .collect())
    }

    /// Уникальные гифки группы (свежие сверху) — для вкладки «Эта группа».
    pub fn gifs_for_group(
        &self,
        group_id: &[u8],
        limit: i64,
    ) -> Result<Vec<crate::proto::Attachment>, CoreError> {
        Ok(match &self.store {
            Some(s) => s
                .gifs_for_group(group_id, limit)?
                .into_iter()
                .map(to_attachment)
                .collect(),
            None => Vec::new(),
        })
    }

    /// Гифки, отправленные конкретным автором (свежие сверху) — для вкладки «Мои».
    pub fn gifs_by_author(
        &self,
        author: &[u8],
        limit: i64,
    ) -> Result<Vec<crate::proto::Attachment>, CoreError> {
        Ok(match &self.store {
            Some(s) => s
                .gifs_by_author(author, limit)?
                .into_iter()
                .map(to_attachment)
                .collect(),
            None => Vec::new(),
        })
    }

    /// Все id сообщений группы (для Merkle-синка).
    pub fn ids_for_group(&self, group_id: &[u8]) -> Vec<MessageId> {
        self.messages
            .values()
            .filter(|m| m.group_id == group_id)
            .map(|m| m.id.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Сообщения группы в причинном порядке (топосортировка + тайбрейк).
    pub fn ordered(&self, group_id: &[u8]) -> Vec<ChatMessage> {
        let mut group: Vec<&ChatMessage> = self
            .messages
            .values()
            .filter(|m| m.group_id == group_id)
            .collect();

        // порядок: сначала меньший lamport, при равенстве — меньший id
        group.sort_by(|a, b| {
            a.lamport
                .cmp(&b.lamport)
                .then_with(|| a.id.cmp(&b.id))
        });

        // Кан-топосорт: узел готов, когда все его родители из группы уже вышли.
        let in_group: HashSet<&MessageId> = group.iter().map(|m| &m.id).collect();
        let mut emitted: HashSet<MessageId> = HashSet::new();
        let mut out = Vec::with_capacity(group.len());
        let mut progress = true;
        while out.len() < group.len() && progress {
            progress = false;
            for m in &group {
                if emitted.contains(&m.id) {
                    continue;
                }
                let ready = m
                    .parents
                    .iter()
                    .all(|p| !in_group.contains(p) || emitted.contains(p));
                if ready {
                    emitted.insert(m.id.clone());
                    out.push((*m).clone());
                    progress = true;
                }
            }
        }
        // на случай оставшихся (битые родители) — добираем как есть
        for m in group {
            if !emitted.contains(&m.id) {
                out.push(m.clone());
            }
        }
        out
    }

    /// Родители, которых у нас нет (нужны для синка).
    pub fn missing_parents(&self, group_id: &[u8]) -> Vec<MessageId> {
        let mut missing = HashSet::new();
        for m in self.messages.values() {
            if m.group_id != group_id {
                continue;
            }
            for p in &m.parents {
                if !self.messages.contains_key(p) {
                    missing.insert(p.clone());
                }
            }
        }
        missing.into_iter().collect()
    }
}

fn to_attachment(a: p2pchat_storage::StoredAttachment) -> crate::proto::Attachment {
    crate::proto::Attachment {
        kind: a.kind,
        id: a.id,
        width: a.width,
        height: a.height,
        url: a.url,
        mime: a.mime,
        size: a.size,
        duration_ms: a.duration_ms,
        file_name: a.file_name,
        thumbnail: a.thumbnail,
    }
}

impl Default for Log {
    fn default() -> Self {
        Self::new()
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_replicas_converge() {
        let mut a = Log::new();
        let mut b = Log::new();
        let g = b"group".to_vec();
        let alice = b"alice".to_vec();
        let bob = b"bob".to_vec();

        let m1 = a.append_local(g.clone(), alice.clone(), "hi".into(), None).unwrap();
        b.merge(m1.clone()).unwrap();
        let m2 = b.append_local(g.clone(), bob.clone(), "hello".into(), None).unwrap();
        a.merge(m2.clone()).unwrap();

        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        assert_eq!(a.ordered(&g), b.ordered(&g));
    }

    #[test]
    fn idempotent_merge() {
        let mut a = Log::new();
        let g = b"g".to_vec();
        let m = a.append_local(g, b"x".to_vec(), "t".into(), None).unwrap();
        let mut b = Log::new();
        assert!(b.merge(m.clone()).unwrap());
        assert!(!b.merge(m).unwrap());
    }
    #[test]
    fn append_ordered_heads_getters() {
        let mut l = Log::new();
        assert!(l.is_empty());
        assert_eq!(l.len(), 0);
        let g = b"g".to_vec();
        let m1 = l.append_local(g.clone(), b"a".to_vec(), "1".into(), None).unwrap();
        let m2 = l.append_local(g.clone(), b"a".to_vec(), "2".into(), None).unwrap();
        assert_eq!(l.len(), 2);
        assert!(l.get(&m1.id).is_some());
        assert!(l.get(b"nope").is_none());
        assert_eq!(l.ordered(&g).len(), 2);
        assert_eq!(l.ids_for_group(&g).len(), 2);
        assert_eq!(l.heads(&g), vec![m2.id.clone()], "голова — последнее");
        assert!(l.tick() > m2.lamport);
    }

    #[test]
    fn missing_parents_tracks_orphans() {
        let mut l = Log::new();
        let g = b"g".to_vec();
        let orphan = ChatMessage {
            id: b"child".to_vec(),
            group_id: g.clone(),
            author: b"a".to_vec(),
            lamport: 5,
            wall_time: 0,
            text: "x".into(),
            parents: vec![b"missing".to_vec()],
            attachment: None,
        };
        l.merge(orphan).unwrap();
        assert_eq!(l.missing_parents(&g), vec![b"missing".to_vec()]);

        let parent = ChatMessage {
            id: b"missing".to_vec(),
            group_id: g.clone(),
            author: b"a".to_vec(),
            lamport: 1,
            wall_time: 0,
            text: "p".into(),
            parents: vec![],
            attachment: None,
        };
        l.merge(parent).unwrap();
        assert!(l.missing_parents(&g).is_empty());
    }

    #[test]
    fn store_wrappers_roundtrip() {
        use p2pchat_storage::Store;
        let st = Store::open_in_memory().unwrap();
        let l = Log::with_store(st).unwrap();

        l.store_group_key(b"g", b"k").unwrap();
        assert_eq!(l.load_group_keys().unwrap().len(), 1);

        l.store_profile(b"p", "Ann", 1, Some(&[1, 2])).unwrap();
        assert_eq!(l.load_profiles().unwrap().len(), 1);

        l.store_edit(b"m", "e", 1, 1).unwrap();
        assert_eq!(l.load_edits().unwrap().len(), 1);

        l.store_delete(b"d").unwrap();
        assert_eq!(l.load_deletes().unwrap(), vec![b"d".to_vec()]);

        let ev = crate::proto::MembershipEvent {
            group_id: b"g".to_vec(),
            peer_id: b"p".to_vec(),
            public_key: b"pk".to_vec(),
            join: true,
            lamport: 1,
            author: b"a".to_vec(),
            id: b"e".to_vec(),
        };
        l.store_membership(&ev).unwrap();
        assert_eq!(l.load_membership().unwrap().len(), 1);

        l.set_meta("k", b"v").unwrap();
        assert_eq!(l.get_meta("k").unwrap().unwrap(), b"v");

        l.set_read(b"g", b"m").unwrap();
        assert_eq!(l.read_marker(b"g").unwrap().unwrap(), b"m");
    }

    #[test]
    fn gifs_wrappers() {
        use p2pchat_storage::Store;
        let st = Store::open_in_memory().unwrap();
        let mut l = Log::with_store(st).unwrap();
        let g = b"g".to_vec();
        let author = b"a".to_vec();
        let att = crate::proto::Attachment {
            kind: "gif".into(),
            id: "abc".into(),
            width: 3,
            height: 4,
            url: String::new(),
            mime: String::new(),
            size: 0,
            duration_ms: 0,
            file_name: String::new(),
            thumbnail: Vec::new(),
        };
        l.append_local(g.clone(), author.clone(), "".into(), Some(att)).unwrap();
        assert_eq!(l.gifs_for_group(&g, 10).unwrap().len(), 1);
        assert_eq!(l.gifs_by_author(&author, 10).unwrap().len(), 1);
        assert_eq!(l.gifs_for_group(b"other", 10).unwrap().len(), 0);
    }

}

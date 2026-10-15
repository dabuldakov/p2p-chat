//! Append-only DAG-лог — источник правды узла.
//!
//! Каждое сообщение ссылается на `parents` (текущие головы группы), образуя
//! направленный ациклический граф. Слияние идемпотентно и коммутативно, а
//! порядок восстанавливается топологической сортировкой с тайбрейком по
//! `(lamport, id)`. Это и есть CRDT: у всех реплик рано или поздно
//! одинаковый результат, независимо от порядка доставки.

use std::collections::{HashMap, HashSet};

use p2pchat_storage::{Store, StoredMessage};

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
}

impl Log {
    pub fn new() -> Self {
        Self {
            clock: Lamport::new(),
            messages: HashMap::new(),
            heads: HashMap::new(),
            has_child: HashSet::new(),
            store: None,
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

    /// Отметка «прочитано до» (id последнего прочитанного сообщения группы).
    pub fn set_read(&self, group: &[u8], id: &[u8]) -> Result<(), CoreError> {
        if let Some(s) = &self.store {
            s.set_read(group, id)?;
        }
        Ok(())
    }

    pub fn read_marker(&self, group: &[u8]) -> Result<Option<Vec<u8>>, CoreError> {
        Ok(match &self.store {
            Some(s) => s.get_read(group)?,
            None => None,
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

impl Default for Log {
    fn default() -> Self {
        Self::new()
    }
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

        let m1 = a.append_local(g.clone(), alice.clone(), "hi".into()).unwrap();
        b.merge(m1.clone()).unwrap();
        let m2 = b.append_local(g.clone(), bob.clone(), "hello".into()).unwrap();
        a.merge(m2.clone()).unwrap();

        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        assert_eq!(a.ordered(&g), b.ordered(&g));
    }

    #[test]
    fn idempotent_merge() {
        let mut a = Log::new();
        let g = b"g".to_vec();
        let m = a.append_local(g, b"x".to_vec(), "t".into()).unwrap();
        let mut b = Log::new();
        assert!(b.merge(m.clone()).unwrap());
        assert!(!b.merge(m).unwrap());
    }
}
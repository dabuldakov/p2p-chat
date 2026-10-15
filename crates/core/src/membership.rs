//! Состав группы — реплицируемое CRDT-множество участников.
//!
//! Каждое событие (`MembershipEvent`) несёт `join`/`leave` и логическое время.
//! Побеждает событие с большим `lamport`, при равенстве — с большим `id`.
//! Из сходящегося состава детерминированно вычисляется топология оверлея
//! (соседи), одинаково у всех узлов.

use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::proto::{MembershipEvent, ProfileEvent};
use crate::PeerId;

/// Малые группы соединяем «каждый с каждым».
pub const MESH_THRESHOLD: usize = 5;

/// Сколько соседей держим в большой группе.
pub const DEFAULT_FANOUT: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub peer_id: PeerId,
    pub public_key: Vec<u8>,
}

#[derive(Debug, Clone)]
struct Entry {
    public_key: Vec<u8>,
    joined: bool,
    lamport: u64,
    id: Vec<u8>,
}

#[derive(Debug, Default, Clone)]
pub struct Membership {
    entries: HashMap<PeerId, Entry>,
}

impl Membership {
    pub fn new() -> Self {
        Self::default()
    }

    /// Применить событие. `true`, если состояние изменилось.
    pub fn apply(&mut self, ev: &MembershipEvent) -> bool {
        let wins = match self.entries.get(&ev.peer_id) {
            None => true,
            Some(cur) => {
                ev.lamport > cur.lamport || (ev.lamport == cur.lamport && ev.id > cur.id)
            }
        };
        if !wins {
            return false;
        }
        self.entries.insert(
            ev.peer_id.clone(),
            Entry {
                public_key: ev.public_key.clone(),
                joined: ev.join,
                lamport: ev.lamport,
                id: ev.id.clone(),
            },
        );
        true
    }

    pub fn is_member(&self, peer_id: &[u8]) -> bool {
        self.entries.get(peer_id).map(|e| e.joined).unwrap_or(false)
    }

    /// Отсортированный список участников (по peer_id).
    pub fn members(&self) -> Vec<Member> {
        let mut v: Vec<Member> = self
            .entries
            .iter()
            .filter(|(_, e)| e.joined)
            .map(|(id, e)| Member { peer_id: id.clone(), public_key: e.public_key.clone() })
            .collect();
        v.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        v
    }

    /// Сколько участников.
    pub fn len(&self) -> usize {
        self.entries.values().filter(|e| e.joined).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn public_key_of(&self, peer_id: &[u8]) -> Option<Vec<u8>> {
        self.entries.get(peer_id).filter(|e| e.joined).map(|e| e.public_key.clone())
    }

    /// Соседи узла `me` в оверлее группы.
    /// * малая группа (≤ MESH_THRESHOLD других) — все остальные (full mesh);
    /// * большая — кольцо с «дальними» связями (offset 1,2,4,8…), до `fanout`.
    pub fn neighbors(&self, me: &[u8], fanout: usize) -> Vec<PeerId> {
        let members: Vec<PeerId> = self.members().into_iter().map(|m| m.peer_id).collect();
        let Some(idx) = members.iter().position(|m| m == me) else {
            return Vec::new();
        };
        let n = members.len();
        let others = n.saturating_sub(1);
        if others <= MESH_THRESHOLD {
            return members.into_iter().filter(|m| m != me).collect();
        }

        let mut out: Vec<PeerId> = Vec::new();
        let mut offset = 1usize;
        while out.len() < fanout && offset < n {
            let fwd = &members[(idx + offset) % n];
            if fwd != me && !out.contains(fwd) {
                out.push(fwd.clone());
            }
            if out.len() >= fanout {
                break;
            }
            let back = &members[(idx + n - offset) % n];
            if back != me && !out.contains(back) {
                out.push(back.clone());
            }
            offset = if offset == 1 { 2 } else { offset * 2 };
        }
        out.truncate(fanout);
        out
    }
}

/// Собрать событие членства (id — контент-хеш).
pub fn membership_event(
    group_id: &[u8],
    peer_id: &[u8],
    public_key: &[u8],
    join: bool,
    lamport: u64,
    author: &[u8],
) -> MembershipEvent {
    let mut ev = MembershipEvent {
        group_id: group_id.to_vec(),
        peer_id: peer_id.to_vec(),
        public_key: public_key.to_vec(),
        join,
        lamport,
        author: author.to_vec(),
        id: Vec::new(),
    };
    let mut h = Sha256::new();
    h.update(&ev.group_id);
    h.update(&ev.peer_id);
    h.update(ev.lamport.to_be_bytes());
    h.update([ev.join as u8]);
    ev.id = h.finalize().to_vec();
    ev
}

/// Собрать событие профиля (никнейм), id — контент-хеш.
pub fn profile_event(peer_id: &[u8], nickname: &str, lamport: u64, author: &[u8]) -> ProfileEvent {
    let mut ev = ProfileEvent {
        peer_id: peer_id.to_vec(),
        nickname: nickname.to_string(),
        lamport,
        author: author.to_vec(),
        id: Vec::new(),
    };
    let mut h = Sha256::new();
    h.update(&ev.peer_id);
    h.update(nickname.as_bytes());
    h.update(ev.lamport.to_be_bytes());
    ev.id = h.finalize().to_vec();
    ev
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(peer: &[u8], join: bool, lamport: u64) -> MembershipEvent {
        membership_event(b"g", peer, &[9], join, lamport, peer)
    }

    #[test]
    fn lww_last_event_wins() {
        let mut m = Membership::new();
        assert!(m.apply(&ev(b"a", true, 1)));
        assert!(m.is_member(b"a"));
        assert!(m.apply(&ev(b"a", false, 2)));
        assert!(!m.is_member(b"a"));
        // старое событие не переигрывает
        assert!(!m.apply(&ev(b"a", true, 1)));
        assert!(!m.is_member(b"a"));
    }

    #[test]
    fn members_sorted() {
        let mut m = Membership::new();
        m.apply(&ev(b"c", true, 1));
        m.apply(&ev(b"a", true, 1));
        m.apply(&ev(b"b", true, 1));
        let ids: Vec<_> = m.members().into_iter().map(|x| x.peer_id).collect();
        assert_eq!(ids, vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    }

    #[test]
    fn small_group_full_mesh() {
        let mut m = Membership::new();
        for p in [b"a".as_slice(), b"b", b"c"] {
            m.apply(&ev(p, true, 1));
        }
        let nb = m.neighbors(b"a", DEFAULT_FANOUT);
        assert_eq!(nb.len(), 2); // все остальные
        assert!(nb.contains(&b"b".to_vec()) && nb.contains(&b"c".to_vec()));
    }

    #[test]
    fn large_group_bounded_fanout() {
        let mut m = Membership::new();
        for i in 0..30u8 {
            m.apply(&ev(&[i], true, 1));
        }
        let nb = m.neighbors(&[0], DEFAULT_FANOUT);
        assert!(nb.len() <= DEFAULT_FANOUT);
        assert!(!nb.contains(&vec![0])); // не сам с собой
        // связность: соседи уникальны
        let mut uniq = nb.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), nb.len());
    }
}
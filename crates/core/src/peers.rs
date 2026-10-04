//! Таблица известных узлов.
//!
//! Слияние — last-write-wins по `last_seen`: побеждает более свежая запись.
//! Таблица распространяется по сети вместе с gossip, поэтому каждый узел
//! постепенно узнаёт всех участников без центрального реестра.

use std::collections::HashMap;

use crate::proto::{Endpoint, PeerInfo, PeerList, Transport};
use crate::PeerId;

pub struct PeerTable {
    local: PeerId,
    peers: HashMap<PeerId, PeerInfo>,
    version: u64,
}

impl PeerTable {
    pub fn new(local: PeerId) -> Self {
        Self { local, peers: HashMap::new(), version: 0 }
    }

    pub fn local_id(&self) -> &PeerId {
        &self.local
    }

    /// Добавить/обновить себя и свои адреса.
    pub fn set_local_endpoints(&mut self, endpoints: Vec<Endpoint>) {
        let entry = self.peers.entry(self.local.clone()).or_insert_with(|| PeerInfo {
            peer_id: self.local.clone(),
            endpoints: Vec::new(),
            last_seen: 0,
            is_relay: false,
            hops: 0,
        });
        entry.endpoints = endpoints;
        entry.last_seen = crate::unix_now();
    }

    pub fn upsert(&mut self, info: PeerInfo) {
        if info.peer_id == self.local {
            return;
        }
        match self.peers.get(&info.peer_id) {
            Some(existing) if existing.last_seen >= info.last_seen => {}
            _ => {
                self.peers.insert(info.peer_id.clone(), info);
            }
        }
    }

    /// Слить удалённый список. Возвращает число реально обновлённых записей.
    pub fn merge(&mut self, list: &PeerList) -> usize {
        let before: usize = self
            .peers
            .values()
            .map(|p| p.last_seen as usize)
            .sum();
        for p in &list.peers {
            self.upsert(p.clone());
        }
        self.version = self.version.max(list.version);
        let after: usize = self
            .peers
            .values()
            .map(|p| p.last_seen as usize)
            .sum();
        (after > before) as usize
    }

    /// Сериализуемый снимок для рассылки.
    pub fn snapshot(&self) -> PeerList {
        PeerList { peers: self.peers.values().cloned().collect(), version: self.version }
    }

    /// Выбрать K соседей для gossip.
    /// Приоритет: relay-узлы, затем случайные. Себя исключаем.
    pub fn gossip_targets(&self, k: usize) -> Vec<PeerId> {
        let now = crate::unix_now();
        let mut candidates: Vec<&PeerInfo> = self
            .peers
            .values()
            .filter(|p| p.peer_id != self.local && now.saturating_sub(p.last_seen) < 300)
            .collect();

        candidates.sort_by_key(|p| (!p.is_relay, p.hops));

        candidates
            .into_iter()
            .take(k)
            .map(|p| p.peer_id.clone())
            .collect()
    }

    pub fn get(&self, id: &[u8]) -> Option<&PeerInfo> {
        self.peers.get(id)
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }

    pub fn all(&self) -> impl Iterator<Item = &PeerInfo> {
        self.peers.values()
    }
}

/// Хелпер: собрать endpoint.
pub fn endpoint(host: &str, port: u16, transport: Transport) -> Endpoint {
    Endpoint { host: host.to_string(), port: port as u32, transport: transport as i32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lww_keeps_newest() {
        let mut t = PeerTable::new(b"self".to_vec());
        t.upsert(PeerInfo {
            peer_id: b"a".to_vec(),
            endpoints: vec![],
            last_seen: 10,
            is_relay: false,
            hops: 1,
        });
        t.upsert(PeerInfo {
            peer_id: b"a".to_vec(),
            endpoints: vec![],
            last_seen: 5,
            is_relay: true,
            hops: 0,
        });
        assert_eq!(t.get(b"a").unwrap().last_seen, 10);
        assert!(!t.get(b"a").unwrap().is_relay);
    }
}
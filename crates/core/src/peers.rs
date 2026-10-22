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
    fn info(id: &[u8], seen: u64, relay: bool, hops: u32) -> PeerInfo {
        PeerInfo {
            peer_id: id.to_vec(),
            endpoints: vec![],
            last_seen: seen,
            is_relay: relay,
            hops,
        }
    }

    #[test]
    fn local_endpoints_and_snapshot() {
        let mut t = PeerTable::new(b"self".to_vec());
        assert_eq!(t.local_id(), b"self");
        t.set_local_endpoints(vec![endpoint("1.2.3.4", 9000, Transport::Udp)]);
        let me = t.get(b"self").unwrap();
        assert_eq!(me.endpoints.len(), 1);
        assert!(me.last_seen > 0);
        let snap = t.snapshot();
        assert!(snap.peers.iter().any(|p| p.peer_id == b"self"));
    }

    #[test]
    fn upsert_ignores_self_and_merge_counts() {
        let mut t = PeerTable::new(b"self".to_vec());
        t.upsert(info(b"self", 100, true, 0));
        assert!(t.get(b"self").is_none(), "чужой upsert себя игнорирует");

        let list = PeerList { peers: vec![info(b"a", 10, false, 1)], version: 5 };
        assert_eq!(t.merge(&list), 1);
        assert_eq!(t.merge(&list), 0, "повтор не обновляет");
        assert_eq!(t.snapshot().version, 5);
    }

    #[test]
    fn gossip_targets_prefers_relay_and_excludes_stale() {
        let mut t = PeerTable::new(b"self".to_vec());
        let now = crate::unix_now();
        t.upsert(info(b"fresh1", now, false, 5));
        t.upsert(info(b"fresh2", now, true, 9));
        t.upsert(info(b"stale", 0, true, 0));

        let targets = t.gossip_targets(2);
        assert_eq!(targets.len(), 2);
        assert!(targets.contains(&b"fresh2".to_vec()), "relay в приоритете");
        assert!(!targets.iter().any(|p| p == b"stale"), "старые (last_seen=0) отсеиваются");
        assert!(!targets.iter().any(|p| p == b"self"));
        assert_eq!(t.gossip_targets(0).len(), 0);
    }

    #[test]
    fn len_get_all_empty() {
        let mut t = PeerTable::new(b"self".to_vec());
        assert!(t.is_empty());
        t.upsert(info(b"a", 1, false, 0));
        assert_eq!(t.len(), 1);
        assert!(!t.is_empty());
        assert_eq!(t.all().count(), 1);
        assert!(t.get(b"zzz").is_none());
        let e = endpoint("h", 1, Transport::Tcp);
        assert_eq!(e.port, 1);
        assert_eq!(e.transport, Transport::Tcp as i32);
    }
}

//! Общие помощники для интеграционных тестов MLS.
#![allow(dead_code)]

use std::collections::{HashSet, VecDeque};

use prost::Message as _;

use p2pchat_core::gossip::Outgoing;
use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;

pub fn engine() -> GossipEngine {
    GossipEngine::new(Identity::generate(), Log::new())
}

pub fn enc(o: &Outgoing) -> Vec<u8> {
    o.envelope.encode_to_vec()
}

/// Доставляет конверты адресатам (по их публичным ключам), узлы пересылают дальше.
pub fn pump(engines: &mut [GossipEngine], keys: &[Vec<u8>], seed: Vec<(usize, Outgoing)>) {
    let n = engines.len();
    let mut queue: VecDeque<(usize, Outgoing)> = seed.into();
    let mut seen: HashSet<(usize, Vec<u8>)> = Default::default();
    let mut guard = 0;
    while let Some((from, out)) = queue.pop_front() {
        guard += 1;
        assert!(guard < 200_000, "gossip did not settle");
        let bytes = out.envelope.encode_to_vec();
        let targets: Vec<usize> = if out.targets.is_empty() {
            (0..n).filter(|&i| i != from).collect()
        } else {
            out.targets
                .iter()
                .filter_map(|pk| keys.iter().position(|k| k == pk))
                .collect()
        };
        for i in targets {
            if i == from || !seen.insert((i, bytes.clone())) {
                continue;
            }
            let outcome = engines[i].handle(&bytes).expect("handle");
            for o in outcome.outgoing {
                queue.push_back((i, o));
            }
        }
    }
}

/// Группа из `n` MLS-участников. Возвращает (движки, публичные ключи, group_id).
pub fn build_group(n: usize) -> (Vec<GossipEngine>, Vec<Vec<u8>>, Vec<u8>) {
    let mut engines: Vec<GossipEngine> = (0..n).map(|_| engine()).collect();
    let g = b"team".to_vec();
    let keys: Vec<Vec<u8>> = engines.iter().map(|e| e.public_key()).collect();
    engines[0].create_group(g.clone()).unwrap();
    for j in 1..n {
        let invite = engines[0].create_invite(&g, 3600, 1).unwrap();
        let (_gid, req) = engines[j].join_request(&invite).unwrap();
        pump(&mut engines, &keys, vec![(j, req)]);
    }
    (engines, keys, g)
}

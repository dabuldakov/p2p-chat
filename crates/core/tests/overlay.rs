//! Оверлей: сообщение расходится по сети (multi-hop) и доходит до всех.

#![allow(clippy::needless_range_loop)]

use std::collections::{HashMap, VecDeque};

use prost::Message as _;

use p2pchat_core::gossip::Outgoing;
use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;

/// Прогоняет gossip: каждый конверт доставляется адресатам; узлы пересылают дальше.
/// Возвращает, от кого каждый узел впервые получил сообщение.
fn gossip(engines: &mut [GossipEngine], keys: &[Vec<u8>], start: usize, first: Outgoing) -> HashMap<usize, usize> {
    let n = engines.len();
    let mut queue: VecDeque<(usize, Outgoing)> = VecDeque::new();
    queue.push_back((start, first));

    let mut got_from: HashMap<usize, usize> = HashMap::new();
    got_from.insert(start, start);

    while let Some((from, out)) = queue.pop_front() {
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
            if i == from {
                continue;
            }
            let outcome = engines[i].handle(&bytes).expect("handle");
            got_from.entry(i).or_insert(from);
            for o in outcome.outgoing {
                queue.push_back((i, o));
            }
        }
    }
    got_from
}

/// Все узлы вступают в группу и получают полный состав (симуляция сходимости).
fn build_group(n: usize) -> (Vec<GossipEngine>, Vec<Vec<u8>>, Vec<u8>) {
    let mut engines: Vec<GossipEngine> =
        (0..n).map(|_| GossipEngine::new(Identity::generate(), Log::new())).collect();
    let g = b"big".to_vec();

    let _key = engines[0].create_group(g.clone());
    let invite = engines[0].invite_for(&g).unwrap();
    for e in engines.iter_mut().skip(1) {
        e.join_from_invite_full(&invite).unwrap();
    }

    // раздаём всем события входа всех — состав сходится
    let joins: Vec<Vec<u8>> = engines
        .iter()
        .map(|e| e.join_envelope(&g).unwrap().unwrap().envelope.encode_to_vec())
        .collect();
    for i in 0..n {
        for j in 0..n {
            if i != j {
                engines[i].handle(&joins[j]).unwrap();
            }
        }
    }

    let keys: Vec<Vec<u8>> = engines.iter().map(|e| e.public_key()).collect();
    (engines, keys, g)
}

#[test]
fn small_group_full_mesh_direct() {
    let (mut engines, keys, g) = build_group(3);
    for e in &engines {
        assert_eq!(e.members(&g).len(), 3);
    }
    let out = engines[0].post(g.clone(), "hi".into()).unwrap();
    let got = gossip(&mut engines, &keys, 0, out);
    assert_eq!(got.len(), 3, "дошло до всех");
}

#[test]
fn large_group_multi_hop_reaches_all() {
    let n = 10;
    let (mut engines, keys, g) = build_group(n);
    for e in &engines {
        assert_eq!(e.members(&g).len(), n);
    }

    // топология не полная: у узла 0 не все прямые соседи
    let direct = engines[0].neighbors(&g).len();
    assert!(direct < n - 1, "ожидался оверлей, а не полный mesh (соседей {direct})");

    let out = engines[0].post(g.clone(), "привет всем".into()).unwrap();
    let got = gossip(&mut engines, &keys, 0, out);

    assert_eq!(got.len(), n, "сообщение дошло до всех {n} узлов");

    // multi-hop: кто-то получил не напрямую от отправителя
    let multi = got.iter().any(|(&i, &f)| i != 0 && f != 0);
    assert!(multi, "сообщение должно было пройти через посредника");

    // у всех одинаковый итоговый лог
    for e in &engines {
        assert_eq!(e.log().ordered(&g).len(), 1);
        assert_eq!(e.log().ordered(&g)[0].text, "привет всем");
    }
}
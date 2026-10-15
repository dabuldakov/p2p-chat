//! Интеграционные сценарии: изоляция групп, выход участника,
//! персистентность состава, сходимость трёх узлов.

#![allow(clippy::needless_range_loop)]

use std::collections::VecDeque;

use prost::Message as _;

use p2pchat_core::gossip::Outgoing;
use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;
use p2pchat_storage::Store;

fn enc(out: &Outgoing) -> Vec<u8> {
    out.envelope.encode_to_vec()
}

/// Доставляет все исходящие по их адресатам, пока не наступит тишина.
fn pump(engines: &mut [GossipEngine], keys: &[Vec<u8>], seed: Vec<(usize, Outgoing)>) {
    let n = engines.len();
    let mut queue: VecDeque<(usize, Outgoing)> = seed.into();
    let mut guard = 0;
    while let Some((from, out)) = queue.pop_front() {
        guard += 1;
        assert!(guard < 20000, "gossip did not settle");
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
            for o in outcome.outgoing {
                queue.push_back((i, o));
            }
        }
    }
}

fn build_group(n: usize) -> (Vec<GossipEngine>, Vec<Vec<u8>>, Vec<u8>) {
    let mut engines: Vec<GossipEngine> =
        (0..n).map(|_| GossipEngine::new(Identity::generate(), Log::new())).collect();
    let g = b"team".to_vec();
    let _key = engines[0].create_group(g.clone());
    let invite = engines[0].invite_for(&g).unwrap();
    for e in engines.iter_mut().skip(1) {
        e.join_from_invite_full(&invite).unwrap();
    }
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
fn groups_are_isolated() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());
    let g1 = b"one".to_vec();
    let g2 = b"two".to_vec();

    let k1 = a.create_group(g1.clone());
    b.join_group(g1.clone(), k1);
    let k2 = a.create_group(g2.clone());
    b.join_group(g2.clone(), k2);

    let e1 = a.post(g1.clone(), "в первой".into()).unwrap();
    b.handle(&enc(&e1)).unwrap();
    assert_eq!(b.log().ordered(&g1).len(), 1);
    assert_eq!(b.log().ordered(&g2).len(), 0, "сообщение не протекло в другую группу");

    let e2 = a.post(g2.clone(), "во второй".into()).unwrap();
    b.handle(&enc(&e2)).unwrap();
    assert_eq!(b.log().ordered(&g2).len(), 1);
    assert_eq!(b.log().ordered(&g1).len(), 1);
}

#[test]
fn member_leave_updates_membership() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());
    let g = b"team".to_vec();

    let _key = a.create_group(g.clone());
    let invite = a.invite_for(&g).unwrap();
    b.join_from_invite_full(&invite).unwrap();

    // обмен входами
    let eb = b.join_envelope(&g).unwrap().unwrap();
    let oa = a.handle(&enc(&eb)).unwrap();
    for o in &oa.outgoing {
        let _ = b.handle(&enc(o)).unwrap();
    }
    assert_eq!(a.members(&g).len(), 2);

    // B выходит
    let leave = b.leave_group(&g).unwrap().unwrap();
    a.handle(&enc(&leave)).unwrap();
    assert_eq!(a.members(&g).len(), 1);
    assert!(!a.members(&g).iter().any(|m| m.peer_id == b.peer_id()));
    assert!(a.neighbors(&g).is_empty());
}

#[test]
fn membership_persists_across_restart() {
    let mut path = std::env::temp_dir();
    path.push(format!("p2pchat-scen-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let path = path.to_str().unwrap().to_string();
    let g = b"team".to_vec();

    // сосед, чей вход мы применим
    let mut b = GossipEngine::new(Identity::generate(), Log::new());

    {
        let store = Store::open(&path).unwrap();
        let identity = Identity::generate();
        store.set_meta("identity", &identity.secret_bytes()).unwrap();
        let mut a = GossipEngine::new(identity, Log::with_store(store).unwrap());
        a.load_state().unwrap();
        a.create_group(g.clone());
        let invite = a.invite_for(&g).unwrap();
        b.join_from_invite_full(&invite).unwrap();
        let eb = b.join_envelope(&g).unwrap().unwrap();
        a.handle(&enc(&eb)).unwrap();
        assert_eq!(a.members(&g).len(), 2);
    }

    // перезапуск
    let store = Store::open(&path).unwrap();
    let identity = Identity::from_secret_bytes(&store.get_meta("identity").unwrap().unwrap()).unwrap();
    let mut a = GossipEngine::new(identity, Log::with_store(store).unwrap());
    a.load_state().unwrap();
    assert_eq!(a.members(&g).len(), 2, "состав сохранился");
    assert_eq!(a.groups(), vec![g.clone()]);
}

#[test]
fn three_nodes_converge_via_sync() {
    let (mut engines, keys, g) = build_group(3);

    // каждый пишет своё, не доставляя
    let _ = engines[0].post(g.clone(), "от 0".into()).unwrap();
    let _ = engines[1].post(g.clone(), "от 1".into()).unwrap();
    let _ = engines[2].post(g.clone(), "от 2".into()).unwrap();

    // каждый сверяется с каждым
    let mut seed = Vec::new();
    for i in 0..3 {
        for j in 0..3 {
            if i != j {
                seed.push((i, engines[i].sync_digest_to(&g, &keys[j]).unwrap()));
            }
        }
    }
    pump(&mut engines, &keys, seed);

    for e in &engines {
        assert_eq!(e.log().ordered(&g).len(), 3, "у каждого все 3 сообщения");
    }
    assert_eq!(engines[0].log().ordered(&g), engines[2].log().ordered(&g));
}
//! Интеграционные сценарии MLS: изоляция групп, удаление участника,
//! персистентность состава, сходимость трёх узлов.

mod common;

use common::*;
use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;
use p2pchat_storage::Store;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-scen-{}-{}.db", std::process::id(), name));
    let _ = std::fs::remove_file(&p);
    p.to_str().unwrap().to_string()
}

#[test]
fn groups_are_isolated() {
    let mut a = engine();
    let mut b = engine();
    let g1 = b"one".to_vec();
    let g2 = b"two".to_vec();

    for g in [g1.clone(), g2.clone()] {
        a.create_group(g.clone()).unwrap();
        let invite = a.create_invite(&g, 3600).unwrap();
        let (_gid, req) = b.join_request(&invite).unwrap();
        let out = a.handle(&enc(&req)).unwrap();
        for o in out.outgoing {
            let _ = b.handle(&enc(&o));
        }
    }

    b.handle(&enc(&a.post(g1.clone(), "в первой".into()).unwrap())).unwrap();
    b.handle(&enc(&a.post(g2.clone(), "во второй".into()).unwrap())).unwrap();
    assert_eq!(b.log().ordered(&g1).len(), 1);
    assert_eq!(b.log().ordered(&g2).len(), 1);
}

#[test]
fn member_removed_updates_membership() {
    let (mut engines, keys, g) = build_group(2);
    assert_eq!(engines[0].members(&g).len(), 2);

    let victim = engines[1].peer_id();
    let outs = engines[0].remove_member(&g, &victim).unwrap();
    pump(&mut engines, &keys, outs.into_iter().map(|o| (0usize, o)).collect());

    assert_eq!(engines[0].members(&g).len(), 1, "удалённый исчез из состава");
    assert!(engines[0].neighbors(&g).is_empty());
}

#[test]
fn membership_persists_across_restart() {
    let path = tmp("scen");
    let g = b"team".to_vec();

    {
        let store = Store::open(&path).unwrap();
        let identity = Identity::generate();
        store.set_meta("identity", &identity.secret_bytes()).unwrap();
        let mut a = GossipEngine::new(identity, Log::with_store(store).unwrap());
        a.load_state().unwrap();
        a.load_mls(&path).unwrap();
        a.create_group(g.clone()).unwrap();

        let mut b = engine();
        let invite = a.create_invite(&g, 3600).unwrap();
        let (_gid, req) = b.join_request(&invite).unwrap();
        let out = a.handle(&enc(&req)).unwrap();
        for o in out.outgoing {
            let _ = b.handle(&enc(&o));
        }
        assert_eq!(a.members(&g).len(), 2);
    }

    // перезапуск
    let store = Store::open(&path).unwrap();
    let identity = Identity::from_secret_bytes(&store.get_meta("identity").unwrap().unwrap()).unwrap();
    let mut a = GossipEngine::new(identity, Log::with_store(store).unwrap());
    a.load_state().unwrap();
    a.load_mls(&path).unwrap();
    assert_eq!(a.members(&g).len(), 2, "состав сохранился");
    assert_eq!(a.groups(), vec![g.clone()]);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn three_nodes_converge_via_sync() {
    let (mut engines, keys, g) = build_group(3);

    let _ = engines[0].post(g.clone(), "от 0".into()).unwrap();
    let _ = engines[1].post(g.clone(), "от 1".into()).unwrap();
    let _ = engines[2].post(g.clone(), "от 2".into()).unwrap();

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
}

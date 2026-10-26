//! Соседи группы: только участники этой группы, без дублей и без чужих групп.

mod common;

use common::*;

#[test]
fn neighbors_are_group_scoped() {
    let (engines, _keys, g) = build_group(4);
    for e in &engines {
        let members = e.members(&g);
        let ids: Vec<Vec<u8>> = members.iter().map(|m| m.peer_id.clone()).collect();
        let ns = e.neighbors(&g);

        assert!(
            ns.len() <= members.len().saturating_sub(1),
            "соседей не больше, чем участников минус я"
        );
        let mut uniq = ns.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), ns.len(), "соседи без дублей");
        assert!(
            ns.iter().all(|n| ids.contains(n)),
            "только участники группы"
        );

        let keys = e.neighbor_keys(&g);
        assert!(
            keys.iter()
                .all(|k| members.iter().any(|m| &m.public_key == k)),
            "ключи соседей — только участники группы"
        );
        assert_eq!(keys.len(), ns.len(), "ключи и соседи совпадают по числу");
    }
}

#[test]
fn neighbor_keys_do_not_cross_groups() {
    let mut a = engine();
    let mut b = engine();
    let mut c = engine();

    let g1 = b"g1".to_vec();
    let g2 = b"g2".to_vec();
    a.create_group(g1.clone()).unwrap();
    a.create_group(g2.clone()).unwrap();

    let inv1 = a.create_invite(&g1, 3600, 1).unwrap();
    let (_g, req1) = b.join_request(&inv1).unwrap();
    let inv2 = a.create_invite(&g2, 3600, 1).unwrap();
    let (_g, req2) = c.join_request(&inv2).unwrap();

    for o in a.handle(&enc(&req1)).unwrap().outgoing {
        let _ = b.handle(&enc(&o));
    }
    for o in a.handle(&enc(&req2)).unwrap().outgoing {
        let _ = c.handle(&enc(&o));
    }

    // У A в каждой группе ровно по одному соседу, и они не пересекаются.
    assert_eq!(a.neighbor_keys(&g1), vec![b.public_key()]);
    assert_eq!(a.neighbor_keys(&g2), vec![c.public_key()]);

    // Сообщение в g1 адресуется только B, но не C (он в другой группе).
    let msg = a.post(g1.clone(), "hi".into()).unwrap();
    assert!(msg.targets.contains(&b.public_key()));
    assert!(!msg.targets.contains(&c.public_key()));
}

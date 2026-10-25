//! Реакции: добавление, замена, снятие, LWW и общий счётчик.

mod common;

use common::*;

fn rid(e: &p2pchat_core::GossipEngine, _g: &[u8], id: &[u8], me: &[u8]) -> Vec<(String, u32, bool)> {
    e.reactions_for(id, me)
        .into_iter()
        .map(|r| (r.emoji, r.count, r.mine))
        .collect()
}

#[test]
fn add_replace_remove_and_count() {
    let (mut engines, keys, g) = build_group(3);
    let msg = engines[0].post(g.clone(), "hello".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, msg)]);
    let id = engines[0].log().ordered(&g).last().unwrap().id.clone();
    let me1 = engines[1].peer_id();
    let me2 = engines[2].peer_id();

    let r1 = engines[1].react(&g, &id, "👍", true).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(1, r1)]);
    for e in &engines {
        assert_eq!(rid(e, &g, &id, &me1), vec![("👍".into(), 1, true)]);
        assert_eq!(rid(e, &g, &id, &me2), vec![("👍".into(), 1, false)]);
    }

    // Замена реакции тем же участником.
    let r2 = engines[1].react(&g, &id, "❤️", true).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(1, r2)]);
    for e in &engines {
        assert_eq!(rid(e, &g, &id, &me1), vec![("❤️".into(), 1, true)]);
        assert!(rid(e, &g, &id, &me1).iter().all(|r| r.0 != "👍"));
    }

    // Второй участник — счётчик 2.
    let r3 = engines[2].react(&g, &id, "❤️", true).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(2, r3)]);
    for e in &engines {
        assert_eq!(rid(e, &g, &id, &me2), vec![("❤️".into(), 2, true)]);
    }

    // Снятие одним участником.
    let r4 = engines[1].react(&g, &id, "❤️", false).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(1, r4)]);
    for e in &engines {
        assert_eq!(rid(e, &g, &id, &me1), vec![("❤️".into(), 1, false)]);
        assert_eq!(rid(e, &g, &id, &me2), vec![("❤️".into(), 1, true)]);
    }
}

#[test]
fn late_old_event_does_not_override_newer() {
    let (mut engines, keys, g) = build_group(3);
    let msg = engines[0].post(g.clone(), "hello".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, msg)]);
    let id = engines[0].log().ordered(&g).last().unwrap().id.clone();

    let old = engines[1].react(&g, &id, "👍", true).unwrap().unwrap();
    let new = engines[1].react(&g, &id, "🔥", true).unwrap().unwrap();
    // Доставляем новый раньше старого.
    let _ = engines[2].handle(&enc(&new)).unwrap();
    let _ = engines[2].handle(&enc(&old)).unwrap();
    assert_eq!(
        rid(&engines[2], &g, &id, &engines[1].peer_id()),
        vec![("🔥".into(), 1, true)]
    );
}

#[test]
fn reject_unknown_or_invalid() {
    let (mut engines, _keys, g) = build_group(2);
    let id = engines[0].log().ordered(&g).last().map(|m| m.id.clone());
    assert!(id.is_none());
    assert!(engines[1].react(&g, b"nope", "👍", true).unwrap().is_none());
    assert!(engines[1].react(&g, b"nope", "", true).unwrap().is_none());
    assert!(engines[1].react(b"other", b"nope", "👍", true).unwrap().is_none());
}

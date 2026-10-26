//! Реакции: добавление, замена, снятие, LWW и общий счётчик.

mod common;

use common::*;

fn rid(
    e: &p2pchat_core::GossipEngine,
    _g: &[u8],
    id: &[u8],
    me: &[u8],
) -> Vec<(String, u32, bool)> {
    e.reactions_for(id, me)
        .into_iter()
        .map(|r| (r.emoji, r.count, r.mine))
        .collect()
}

#[test]
fn add_multiple_and_remove() {
    let (mut engines, keys, g) = build_group(3);
    let msg = engines[0].post(g.clone(), "hello".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, msg)]);
    let id = engines[0].log().ordered(&g).last().unwrap().id.clone();
    let me1 = engines[1].peer_id();
    let me2 = engines[2].peer_id();

    let r1 = engines[1].react(&g, &id, "👍", true).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(1, r1)]);
    // Второй участник ставит другую — обе сохраняются.
    let r2 = engines[2].react(&g, &id, "❤️", true).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(2, r2)]);
    for e in &engines {
        let mine = rid(e, &g, &id, &me1);
        assert!(mine.contains(&("👍".into(), 1, true)));
        assert!(mine.contains(&("❤️".into(), 1, false)));
        let other = rid(e, &g, &id, &me2);
        assert!(other.contains(&("❤️".into(), 1, true)));
        assert!(other.contains(&("👍".into(), 1, false)));
    }

    // Один участник может поставить несколько реакций на одно сообщение.
    let r3 = engines[1].react(&g, &id, "🔥", true).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(1, r3)]);
    for e in &engines {
        let mine = rid(e, &g, &id, &me1);
        assert!(mine.contains(&("👍".into(), 1, true)));
        assert!(mine.contains(&("🔥".into(), 1, true)));
    }

    // Убираем только одну свою реакцию, остальные остаются.
    let r4 = engines[1].react(&g, &id, "👍", false).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(1, r4)]);
    for e in &engines {
        let mine = rid(e, &g, &id, &me1);
        assert!(mine.iter().all(|r| r.0 != "👍"));
        assert!(mine.contains(&("🔥".into(), 1, true)));
        assert!(mine.contains(&("❤️".into(), 1, false)));
    }

    // Повторное снятие/постановка уже не меняет состояние.
    assert!(engines[1].react(&g, &id, "👍", false).unwrap().is_none());
    assert!(engines[1].react(&g, &id, "🔥", true).unwrap().is_none());
}

#[test]
fn multiple_reactors_count_together() {
    let (mut engines, keys, g) = build_group(3);
    let msg = engines[0].post(g.clone(), "hello".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, msg)]);
    let id = engines[0].log().ordered(&g).last().unwrap().id.clone();

    let a = engines[1].react(&g, &id, "❤️", true).unwrap().unwrap();
    let b = engines[2].react(&g, &id, "❤️", true).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(1, a), (2, b)]);
    for e in &engines {
        assert_eq!(
            rid(e, &g, &id, &engines[1].peer_id()),
            vec![("❤️".into(), 2, true)]
        );
        assert_eq!(
            rid(e, &g, &id, &engines[2].peer_id()),
            vec![("❤️".into(), 2, true)]
        );
        assert_eq!(
            rid(e, &g, &id, &engines[0].peer_id()),
            vec![("❤️".into(), 2, false)]
        );
    }
}

#[test]
fn reject_unknown_or_invalid() {
    let (mut engines, _keys, g) = build_group(2);
    let id = engines[0].log().ordered(&g).last().map(|m| m.id.clone());
    assert!(id.is_none());
    assert!(engines[1].react(&g, b"nope", "👍", true).unwrap().is_none());
    assert!(engines[1].react(&g, b"nope", "", true).unwrap().is_none());
    assert!(engines[1]
        .react(b"other", b"nope", "👍", true)
        .unwrap()
        .is_none());
}

#[test]
fn limit_100_reactions_per_user() {
    let (mut engines, keys, g) = build_group(2);
    let msg = engines[1].post(g.clone(), "hello".into()).unwrap();
    pump(&mut engines, &keys, vec![(1, msg)]);
    let id = engines[0].log().ordered(&g).last().unwrap().id.clone();

    for i in 0..100 {
        assert!(
            engines[1]
                .react(&g, &id, &format!("e{i}"), true)
                .unwrap()
                .is_some(),
            "reaction {i}"
        );
    }
    assert!(engines[1]
        .react(&g, &id, "overflow", true)
        .unwrap()
        .is_none());
    assert!(engines[1].react(&g, &id, "e0", false).unwrap().is_some());
    assert!(engines[1]
        .react(&g, &id, "overflow", true)
        .unwrap()
        .is_some());
}

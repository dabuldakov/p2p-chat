//! Оверлей: сообщение расходится по сети (multi-hop) и доходит до всех.

mod common;

use common::*;

#[test]
fn small_group_full_mesh_direct() {
    let (mut engines, keys, g) = build_group(3);
    for e in &engines {
        assert_eq!(e.members(&g).len(), 3);
    }
    let out = engines[0].post(g.clone(), "hi".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    for e in &engines {
        assert_eq!(e.log().ordered(&g).len(), 1, "дошло до всех");
    }
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
    pump(&mut engines, &keys, vec![(0, out)]);

    for e in &engines {
        let log = e.log().ordered(&g);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].text, "привет всем");
    }
}

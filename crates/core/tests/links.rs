//! Эфемерные публикации связей: доставляются соседям, НЕ попадают в лог.

mod common;

use common::*;

#[test]
fn links_are_published_and_not_persisted() {
    let (mut engines, keys, g) = build_group(2);
    let len_before = engines[1].log().ordered(&g).len();

    let b_peer = engines[1].peer_id();
    let out = engines[0]
        .post_links(&g, vec![(b_peer.clone(), "direct".into())])
        .unwrap()
        .expect("outgoing");
    pump(&mut engines, &keys, vec![(0, out)]);

    // B видит связь A→B как direct
    let links = engines[1].links(&g);
    assert!(
        links.iter().any(|(a, p, k)| *a == engines[0].peer_id() && *p == b_peer && k == "direct"),
        "связь не доставлена: {links:?}"
    );
    // и своя (A) тоже сохранена
    assert!(engines[0].links(&g).iter().any(|(a, _, _)| *a == engines[0].peer_id()));

    // в лог не попало (эфемерно)
    assert_eq!(engines[1].log().ordered(&g).len(), len_before, "links не должны писаться в лог");
}

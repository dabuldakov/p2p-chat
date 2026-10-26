//! Merkle-синк: две реплики разошлись, digest-протокол их сводит.

mod common;

use common::*;
use p2pchat_core::gossip::Outgoing;
use prost::Message as _;

#[test]
fn diverged_replicas_converge_via_merkle() {
    let (mut e, keys, g) = build_group(2);

    // A пишет a1 и доставляет B; B пишет b1, но не доставляет.
    let a1 = e[0].post(g.clone(), "a1".into()).unwrap();
    pump(&mut e, &keys, vec![(0, a1)]);
    let _b1 = e[1].post(g.clone(), "b1".into()).unwrap();

    // A инициирует синк (broadcast digest)
    let digest = e[0].sync_digest(&g).unwrap();
    pump(
        &mut e,
        &keys,
        vec![(
            0,
            Outgoing {
                envelope: digest,
                targets: vec![],
            },
        )],
    );

    assert_eq!(e[0].log().len(), 2, "A добрал b1");
    assert_eq!(e[1].log().len(), 2, "B уже имел оба");
    assert_eq!(
        e[0].log().ordered(&g),
        e[1].log().ordered(&g),
        "порядок совпал"
    );
}

#[test]
fn already_synced_is_noop() {
    let (mut e, keys, g) = build_group(2);

    let a1 = e[0].post(g.clone(), "hi".into()).unwrap();
    pump(&mut e, &keys, vec![(0, a1)]);

    let digest = e[0].sync_digest(&g).unwrap();
    let out = e[1].handle(&digest.encode_to_vec()).unwrap();
    assert!(out.outgoing.is_empty(), "корни совпали — синк не нужен");
}

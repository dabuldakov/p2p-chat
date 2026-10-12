//! Проверка Merkle-синка: две реплики разошлись, digest-протокол их сводит.

use prost::Message as _;

use p2pchat_core::gossip::Outgoing;
use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;

fn enc(out: &Outgoing) -> Vec<u8> {
    out.envelope.encode_to_vec()
}

/// Прогоняет конверты между двумя движками, пока не наступит тишина.
fn pump(a: &mut GossipEngine, b: &mut GossipEngine, seed_to_b: Vec<Vec<u8>>) {
    let mut to_b = seed_to_b;
    let mut to_a: Vec<Vec<u8>> = Vec::new();

    for _ in 0..20 {
        let mut next_a = Vec::new();
        let mut next_b = Vec::new();

        for e in to_b.drain(..) {
            let o = b.handle(&e).expect("b handle");
            next_a.extend(o.outgoing.iter().map(enc));
        }
        for e in to_a.drain(..) {
            let o = a.handle(&e).expect("a handle");
            next_b.extend(o.outgoing.iter().map(enc));
        }

        to_a = next_a;
        to_b = next_b;
        if to_a.is_empty() && to_b.is_empty() {
            return;
        }
    }
    panic!("sync did not settle");
}

#[test]
fn diverged_replicas_converge_via_merkle() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());
    let g = b"group".to_vec();

    let a1 = a.post(g.clone(), "a1".into()).unwrap();
    b.handle(&enc(&a1)).unwrap();

    let _b1 = b.post(g.clone(), "b1".into()).unwrap();

    let digest = a.sync_digest(&g).unwrap();
    pump(&mut a, &mut b, vec![digest.encode_to_vec()]);

    assert_eq!(a.log().len(), 2, "A добрал b1");
    assert_eq!(b.log().len(), 2, "B уже имел оба");
    assert_eq!(a.log().ordered(&g), b.log().ordered(&g), "порядок совпал");
}

#[test]
fn already_synced_is_noop() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());
    let g = b"group".to_vec();

    let a1 = a.post(g.clone(), "hi".into()).unwrap();
    b.handle(&enc(&a1)).unwrap();

    let digest = a.sync_digest(&g).unwrap();
    let out = b.handle(&digest.encode_to_vec()).unwrap();
    assert!(out.outgoing.is_empty(), "корни совпали — синк не нужен");
}

//! Профиль: никнейм реплицируется по сети и обновляется по LWW.

use prost::Message as _;

use p2pchat_core::gossip::Outgoing;
use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;

fn enc(out: &Outgoing) -> Vec<u8> {
    out.envelope.encode_to_vec()
}

#[test]
fn nickname_propagates_and_lww() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());

    let out = a.set_nickname("Алиса").unwrap().unwrap();
    b.handle(&enc(&out)).unwrap();
    assert_eq!(b.nickname_of(&a.peer_id()).as_deref(), Some("Алиса"));

    // обновление никнейма — побеждает более позднее (lamport)
    let out2 = a.set_nickname("Алиса-2").unwrap().unwrap();
    b.handle(&enc(&out2)).unwrap();
    assert_eq!(b.nickname_of(&a.peer_id()).as_deref(), Some("Алиса-2"));

    // чужой ник неизвестен
    assert_eq!(b.nickname_of(b"nope"), None);
}

#[test]
fn empty_profile_when_not_set() {
    let a = GossipEngine::new(Identity::generate(), Log::new());
    assert_eq!(a.my_nickname(), None);
    assert!(a.profile_envelope().unwrap().is_none());
}

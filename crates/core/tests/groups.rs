//! Группы: создание, инвайт, вступление, состав и соседи.

use prost::Message as _;

use p2pchat_core::gossip::Outgoing;
use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;

fn enc(out: &Outgoing) -> Vec<u8> {
    out.envelope.encode_to_vec()
}

#[test]
fn create_join_and_membership_converges() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());
    let g = b"team".to_vec();

    // A создаёт группу и получает инвайт
    let _key = a.create_group(g.clone());
    let invite = a.invite_for(&g).expect("invite");
    assert!(invite.starts_with("p2pchat://join?g="));

    // B вступает по инвайту
    let gid = b.join_from_invite(&invite).expect("join");
    assert_eq!(gid, g);
    assert!(b.has_group(&g));

    // Обмен событиями входа
    let ea = a.join_envelope(&g).unwrap().unwrap();
    b.handle(&enc(&ea)).unwrap();
    let eb = b.join_envelope(&g).unwrap().unwrap();
    a.handle(&enc(&eb)).unwrap();

    // Оба видят полный состав из 2 человек
    assert_eq!(a.members(&g).len(), 2);
    assert_eq!(b.members(&g).len(), 2);

    let a_ids: Vec<_> = a.members(&g).into_iter().map(|m| m.peer_id).collect();
    assert!(a_ids.contains(&b.peer_id()));

    // Малая группа — full mesh: у каждого ровно 1 сосед
    assert_eq!(a.neighbors(&g).len(), 1);
    assert_eq!(b.neighbors(&g).len(), 1);
    assert_eq!(a.neighbors(&g)[0], b.peer_id());
}

#[test]
fn invite_roundtrip() {
    use p2pchat_core::gossip::parse_invite;
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let g = b"x".to_vec();
    let key = a.create_group(g.clone());
    let invite = a.invite_for(&g).unwrap();
    let (gid, k) = parse_invite(&invite).unwrap();
    assert_eq!(gid, g);
    assert_eq!(k, key);
}

#[test]
fn bootstrap_mutual_discovery() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());
    let g = b"team".to_vec();

    let _key = a.create_group(g.clone());
    let invite = a.invite_for(&g).unwrap();

    // B вступает по инвайту и узнаёт ключ хозяина
    let (gid, host) = b.join_from_invite_full(&invite).unwrap();
    assert_eq!(gid, g);
    assert_eq!(host, a.public_key(), "инвайт содержит ключ хозяина");

    // B отправляет свой вход, A его принимает
    let eb = b.join_envelope(&g).unwrap().unwrap();
    let oa = a.handle(&enc(&eb)).unwrap();

    // A узнал B (2 участника) и ответил своим входом именно B
    assert_eq!(a.members(&g).len(), 2);
    let to_b = oa.outgoing.iter().any(|o| o.targets.contains(&b.public_key()));
    assert!(to_b, "A должен ответить своему новому соседу");

    // B применяет ответ A -> теперь и B видит A
    for o in oa.outgoing.iter() {
        b.handle(&enc(o)).unwrap();
    }
    assert_eq!(b.members(&g).len(), 2);
    assert!(b.members(&g).iter().any(|m| m.peer_id == a.peer_id()));
    assert!(a.members(&g).iter().any(|m| m.peer_id == b.peer_id()));
}
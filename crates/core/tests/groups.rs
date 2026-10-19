//! Группы (MLS): создание, впуск по билету, состав и соседи.

mod common;

use common::*;
use p2pchat_core::gossip::parse_invite;

#[test]
fn create_join_and_membership_converges() {
    let (engines, _keys, g) = build_group(2);
    for e in &engines {
        assert_eq!(e.members(&g).len(), 2, "оба участника видят полный состав");
        assert_eq!(e.neighbors(&g).len(), 1, "малая группа — full mesh");
    }
    let a_ids: Vec<_> = engines[0].members(&g).into_iter().map(|m| m.peer_id).collect();
    assert!(a_ids.contains(&engines[1].peer_id()));
}

#[test]
fn invite_ticket_roundtrip() {
    let mut a = engine();
    let g = b"x".to_vec();
    a.create_group(g.clone()).unwrap();
    let invite = a.create_invite(&g, 3600, 1).unwrap();
    assert!(invite.starts_with("p2pchat://join?g="));

    let (gid, host) = parse_invite(&invite).unwrap();
    assert_eq!(gid, g);
    assert_eq!(host, a.public_key(), "в билете публичный ключ хозяина");

    let inv = p2pchat_core::invite::Invite::parse(&invite).unwrap();
    assert!(inv.verify_sig(), "подпись хозяина валидна");
}

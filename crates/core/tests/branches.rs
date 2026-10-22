//! Дополнительные ветки: выход из группы, не-владелец, пустой профиль.

mod common;

use common::*;

#[test]
fn leave_group_removes_and_is_idempotent() {
    let mut e = engine();
    let g = e.create_named_group("G").unwrap();
    assert!(e.has_group(&g));
    assert!(e.leave_group(&g).unwrap().is_none());
    assert!(!e.has_group(&g));
    assert!(e.leave_group(&g).unwrap().is_none(), "повторный выход — no-op");
    assert!(e.groups().is_empty());
}

#[test]
fn non_owner_group_ops_are_noops() {
    let (mut engines, _keys, g) = build_group(2);
    assert!(engines[1].set_group_avatar(&g, vec![1]).unwrap().is_none());
    assert!(engines[1].set_group_name(&g, "X".into()).unwrap().is_none());
    let owner = engines[0].peer_id();
    assert!(engines[1].remove_member(&g, &owner).unwrap().is_empty());
}

#[test]
fn empty_profile_and_bad_inputs() {
    let mut e = engine();
    assert!(!e.is_owner(b"nope").unwrap());
    assert_eq!(e.my_nickname(), None);
    assert!(e.profile_envelope().unwrap().is_none());
    assert!(e.create_invite(b"nope", 60, 1).is_err());
    assert!(e.join_request("p2pchat://join?g=00").is_err());
    assert!(e.leave_group(b"nope").unwrap().is_none());
}

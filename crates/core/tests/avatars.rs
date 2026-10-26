//! Аватары: профиль (рассылается) и группы (только владелец).

mod common;

use common::*;

#[test]
fn profile_avatar_propagates() {
    let mut a = engine();
    let mut b = engine();
    let av = vec![1u8, 2, 3, 4, 5];
    let out = a.set_avatar(av.clone()).unwrap().expect("outgoing");
    b.handle(&enc(&out)).unwrap();
    assert_eq!(b.avatar_of(&a.peer_id()), Some(av));
}

#[test]
fn group_avatar_only_owner() {
    let (mut engines, keys, g) = build_group(2);
    let av = vec![9u8, 8, 7, 6];

    // не-владелец не может
    assert!(engines[1]
        .set_group_avatar(&g, vec![0u8])
        .unwrap()
        .is_none());

    // владелец (создатель, индекс 0) может
    let out = engines[0]
        .set_group_avatar(&g, av.clone())
        .unwrap()
        .expect("outgoing");
    pump(&mut engines, &keys, vec![(0, out)]);
    assert_eq!(engines[1].group_avatar(&g), Some(av));
    assert_eq!(engines[0].group_avatar(&g), Some(vec![9u8, 8, 7, 6]));
}

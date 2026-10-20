//! Удаление сообщений: tombstone, только автор, применяется у всех.

mod common;

use common::*;

#[test]
fn delete_propagates_and_only_author() {
    let (mut engines, keys, g) = build_group(2);

    let m = engines[0].post(g.clone(), "secret".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, m)]);
    let id = engines[0].log().ordered(&g)[0].id.clone();

    // не-автор не может удалить
    assert!(engines[1].post_delete(&g, &id).unwrap().is_none());

    let del = engines[0].post_delete(&g, &id).unwrap().expect("outgoing");
    pump(&mut engines, &keys, vec![(0, del)]);

    assert!(engines[0].is_deleted(&id));
    assert!(engines[1].is_deleted(&id), "удаление доехало до участника");
}

#[test]
fn rename_propagates_and_only_owner() {
    let mut host = engine();
    let mut joiner = engine();
    let g = host.create_named_group("Old name").unwrap();

    let invite = host.create_invite(&g, 3600, 1).unwrap();
    let (jg, req) = joiner.join_request(&invite).unwrap();
    assert_eq!(jg, g);
    for o in host.handle(&enc(&req)).unwrap().outgoing {
        let _ = joiner.handle(&enc(&o));
    }
    assert_eq!(joiner.group_name(&g), "Old name");

    // не-владелец переименовать не может
    assert!(joiner.set_group_name(&g, "Hacked".into()).unwrap().is_none());

    // владелец меняет имя; аватар при этом не теряется
    let av = host.set_group_avatar(&g, vec![1, 2, 3]).unwrap().expect("avatar");
    let _ = joiner.handle(&enc(&av));
    assert_eq!(joiner.group_avatar(&g), Some(vec![1, 2, 3]));

    let out = host.set_group_name(&g, "New name".into()).unwrap().expect("rename");
    let _ = joiner.handle(&enc(&out));

    assert_eq!(host.group_name(&g), "New name");
    assert_eq!(joiner.group_name(&g), "New name");
    assert_eq!(joiner.group_avatar(&g), Some(vec![1, 2, 3]), "имя не стирает аватар");
}

//! Блокировка пользователей: скрытие из непрочитанных и сохранение.

mod common;

use common::*;

#[test]
fn block_hides_unread_and_can_unblock() {
    let (mut engines, keys, g) = build_group(2);
    let m = engines[0].post(g.clone(), "hi".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, m)]);

    let author = engines[0].peer_id();
    assert!(!engines[1].is_blocked(&author));
    assert_eq!(engines[1].unread_count(&g).unwrap(), 1);

    engines[1].block_user(&author);
    assert!(engines[1].is_blocked(&author));
    assert_eq!(engines[1].blocked_users(), vec![author.clone()]);
    assert_eq!(engines[1].unread_count(&g).unwrap(), 0, "сообщения скрыты");

    engines[1].unblock_user(&author);
    assert!(!engines[1].is_blocked(&author));
    assert_eq!(engines[1].unread_count(&g).unwrap(), 1, "снова видно");
}

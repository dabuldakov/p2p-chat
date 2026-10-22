//! Непрочитанные: отметка «прочитано до», счётчик по группе.

mod common;

use common::*;

#[test]
fn empty_group_has_zero_unread() {
    let mut e = engine();
    let g = e.create_named_group("U").unwrap();
    assert_eq!(e.unread_count(&g).unwrap(), 0);
}

#[test]
fn all_messages_unread_without_marker_then_mark_read_clears() {
    let mut e = engine();
    let g = e.create_named_group("U").unwrap();
    e.post(g.clone(), "a".into()).unwrap();
    e.post(g.clone(), "b".into()).unwrap();
    assert_eq!(e.unread_count(&g).unwrap(), 2, "без отметки — все непрочитанные");

    e.mark_read(&g).unwrap();
    assert_eq!(e.unread_count(&g).unwrap(), 0);

    e.post(g.clone(), "c".into()).unwrap();
    assert_eq!(e.unread_count(&g).unwrap(), 1, "новое после отметки");
}

#[test]
fn mark_read_on_empty_is_noop() {
    let mut e = engine();
    let g = e.create_named_group("U").unwrap();
    e.mark_read(&g).unwrap();
    assert_eq!(e.unread_count(&g).unwrap(), 0);
    e.post(g.clone(), "a".into()).unwrap();
    assert_eq!(e.unread_count(&g).unwrap(), 1, "отметка не появилась — считаем всё");
}

#[test]
fn unknown_marker_counts_all() {
    let mut e = engine();
    let g = e.create_named_group("U").unwrap();
    e.post(g.clone(), "a".into()).unwrap();
    e.log().set_read(&g, b"missing").unwrap();
    assert_eq!(e.unread_count(&g).unwrap(), 1);
}

#[test]
fn unread_is_per_group() {
    let mut e = engine();
    let g1 = e.create_named_group("A").unwrap();
    let g2 = e.create_named_group("B").unwrap();
    e.post(g1.clone(), "a".into()).unwrap();
    e.post(g2.clone(), "b".into()).unwrap();
    e.post(g2.clone(), "c".into()).unwrap();
    e.mark_read(&g1).unwrap();

    assert_eq!(e.unread_count(&g1).unwrap(), 0);
    assert_eq!(e.unread_count(&g2).unwrap(), 2);
}

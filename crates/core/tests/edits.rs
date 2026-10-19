//! Правки сообщений: применяются у всех, правит только автор (LWW).

mod common;

use common::*;

#[test]
fn edit_propagates_and_only_author() {
    let (mut engines, keys, g) = build_group(2);

    let m = engines[0].post(g.clone(), "hello".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, m)]);

    let mid = engines[0].log().ordered(&g)[0].id.clone();

    // редактирует автор (A)
    let out = engines[0]
        .post_edit(&g, &mid, "hello (edited)".into())
        .unwrap()
        .expect("outgoing");
    pump(&mut engines, &keys, vec![(0, out)]);

    // B видит новый текст как правку
    let base = engines[1].log().get(&mid).unwrap().text.clone();
    let (text, edited) = engines[1].effective_text(&mid, &base);
    assert_eq!(text, "hello (edited)");
    assert!(edited, "отмечено как отредактированное");

    // не-автор не может править
    assert!(engines[1]
        .post_edit(&g, &mid, "hacked".into())
        .unwrap()
        .is_none());
}

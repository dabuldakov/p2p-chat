//! Коммиты MLS принимаются только от владельца (проверка до merge).

use p2pchat_mls::*;

#[test]
fn non_owner_commit_is_rejected_without_merge() {
    let alice = new_identity(b"alice-account").unwrap();
    let bob = new_identity(b"bob-account").unwrap();
    let charlie = new_identity(b"charlie-account").unwrap();

    let mut alice_group = create_group(&alice, b"team").unwrap();
    let (_c1, w1) = add_member(&mut alice_group, &alice, &key_package(&bob).unwrap()).unwrap();
    let mut bob_group = join(&bob, &w1).unwrap();

    // Bob — не владелец, но MLS технически позволяет ему сделать Commit.
    let (bad_commit, _w2) =
        add_member(&mut bob_group, &bob, &key_package(&charlie).unwrap()).unwrap();
    assert_eq!(member_count(&bob_group), 3);

    // Alice отклоняет Commit: состояние группы не меняется.
    let err = process_checked(
        &mut alice_group,
        &alice,
        &bad_commit,
        Some(b"alice-account"),
    );
    assert!(err.is_err(), "Commit от Bob должен быть отклонён");
    assert_eq!(member_count(&alice_group), 2, "merge не произошёл");
}

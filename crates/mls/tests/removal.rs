//! Пять участников: впуск нескольких, удаление и forward secrecy (удалённый не
//! читает новые сообщения; оставшиеся читают).

use p2pchat_mls::*;

#[test]
fn add_remove_and_forward_secrecy() {
    let alice = new_identity(b"alice-account").unwrap();
    let bob = new_identity(b"bob-account").unwrap();
    let charlie = new_identity(b"charlie-account").unwrap();

    // Alice создаёт группу.
    let mut alice_group = create_group(&alice, b"team").unwrap();
    assert_eq!(member_count(&alice_group), 1);

    // Впускаем Bob.
    let (c1, w1) = add_member(&mut alice_group, &alice, &key_package(&bob).unwrap()).unwrap();
    assert_eq!(member_count(&alice_group), 2);
    let _ = c1; // другим участникам пока некому слать
    let mut bob_group = join(&bob, &w1).unwrap();

    // Впускаем Charlie.
    let (c2, w2) = add_member(&mut alice_group, &alice, &key_package(&charlie).unwrap()).unwrap();
    assert_eq!(member_count(&alice_group), 3);
    // Bob применяет commit о добавлении Charlie.
    assert_eq!(
        process(&mut bob_group, &bob, &c2).unwrap(),
        Received::Handshake
    );
    assert_eq!(member_count(&bob_group), 3);
    let mut charlie_group = join(&charlie, &w2).unwrap();
    assert_eq!(member_count(&charlie_group), 3);

    // Alice удаляет Bob.
    let c3 = remove_member(&mut alice_group, &alice, b"bob-account").unwrap();
    assert_eq!(member_count(&alice_group), 2);
    // Charlie применяет удаление.
    assert_eq!(
        process(&mut charlie_group, &charlie, &c3).unwrap(),
        Received::Handshake
    );
    assert_eq!(member_count(&charlie_group), 2);
    // Bob применяет удаление себя → группа становится неактивной.
    let _ = process(&mut bob_group, &bob, &c3);

    // Новое сообщение: Charlie читает, Bob — нет (forward secrecy).
    let ct = encrypt(&mut alice_group, &alice, b"after removal").unwrap();
    assert_eq!(
        process(&mut charlie_group, &charlie, &ct).unwrap(),
        Received::Application(b"after removal".to_vec())
    );
    assert!(process(&mut bob_group, &bob, &ct).is_err());
}

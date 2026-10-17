//! Сквозной сценарий на два участника: создание группы, впуск по KeyPackage +
//! Welcome, обмен прикладными сообщениями.

use p2pchat_mls::*;

#[test]
fn two_party_group_roundtrip() {
    let alice = new_identity(b"alice-account").expect("alice");
    let bob = new_identity(b"bob-account").expect("bob");

    // Bob публикует KeyPackage; Alice создаёт группу и впускает его.
    let bob_kp = key_package(&bob).expect("kp");
    let mut alice_group = create_group(&alice, b"team").expect("group");
    let (_commit, welcome) = add_member(&mut alice_group, &alice, &bob_kp).expect("add");

    // Bob входит по Welcome.
    let mut bob_group = join(&bob, &welcome).expect("join");

    // Alice -> Bob
    let ct1 = encrypt(&mut alice_group, &alice, b"hi bob").expect("enc1");
    assert_eq!(
        decrypt(&mut bob_group, &bob, &ct1).expect("dec1").as_deref(),
        Some(&b"hi bob"[..])
    );

    // Bob -> Alice
    let ct2 = encrypt(&mut bob_group, &bob, b"hi alice").expect("enc2");
    assert_eq!(
        decrypt(&mut alice_group, &alice, &ct2).expect("dec2").as_deref(),
        Some(&b"hi alice"[..])
    );
}

#[test]
fn third_party_cannot_decrypt() {
    let alice = new_identity(b"alice").unwrap();
    let bob = new_identity(b"bob").unwrap();
    let eve = new_identity(b"eve").unwrap();

    let bob_kp = key_package(&bob).unwrap();
    let mut alice_group = create_group(&alice, b"secret").unwrap();
    let (_c, welcome) = add_member(&mut alice_group, &alice, &bob_kp).unwrap();
    let mut bob_group = join(&bob, &welcome).unwrap();

    let ct = encrypt(&mut alice_group, &alice, b"top secret").unwrap();
    // Eve не состоит в группе — расшифровать нечем (нет группы/ключей).
    let mut eve_group = create_group(&eve, b"other").unwrap();
    assert!(decrypt(&mut eve_group, &eve, &ct).is_err());
    // Bob читает.
    assert_eq!(
        decrypt(&mut bob_group, &bob, &ct).unwrap().as_deref(),
        Some(&b"top secret"[..])
    );
}

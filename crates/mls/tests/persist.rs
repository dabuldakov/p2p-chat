//! Персистентность: после «перезапуска» (перезагрузки состояния из SQLite)
//! группа и переписка продолжают работать.

use p2pchat_mls::*;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-mls-{}-{}.db", std::process::id(), name));
    let _ = std::fs::remove_file(&p);
    p.to_str().unwrap().to_string()
}

#[test]
fn group_survives_restart() {
    let alice_db = tmp("alice");
    let bob_db = tmp("bob");

    let alice = new_identity(b"alice-account").expect("alice");
    let bob = new_identity(b"bob-account").expect("bob");

    let bob_kp = key_package(&bob).expect("kp");
    let mut alice_group = create_group(&alice, b"team").expect("group");
    let (_commit, welcome) = add_member(&mut alice_group, &alice, &bob_kp).expect("add");
    let mut bob_group = join(&bob, &welcome).expect("join");

    // Обмен до перезапуска.
    let ct1 = encrypt(&mut alice_group, &alice, b"before restart").expect("e1");
    assert_eq!(
        decrypt(&mut bob_group, &bob, &ct1).expect("d1").as_deref(),
        Some(&b"before restart"[..])
    );

    // Сохраняем состояние обоих в отдельные БД.
    save(&alice, &alice_db).expect("save alice");
    save(&bob, &bob_db).expect("save bob");
    drop(alice_group);
    drop(bob_group);
    drop(alice);
    drop(bob);

    // «Перезапуск»: поднимаем личности и группы из БД.
    let alice2 = load_identity(b"alice-account", &alice_db).expect("load alice");
    let bob2 = load_identity(b"bob-account", &bob_db).expect("load bob");
    let mut alice_group2 = load_group(&alice2, b"team").expect("lg a").expect("alice group");
    let mut bob_group2 = load_group(&bob2, b"team").expect("lg b").expect("bob group");

    // Продолжаем переписку после перезапуска.
    let ct2 = encrypt(&mut alice_group2, &alice2, b"after restart").expect("e2");
    assert_eq!(
        decrypt(&mut bob_group2, &bob2, &ct2).expect("d2").as_deref(),
        Some(&b"after restart"[..])
    );

    let _ = std::fs::remove_file(&alice_db);
    let _ = std::fs::remove_file(&bob_db);
}

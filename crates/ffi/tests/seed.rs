//! Бэкап/восстановление идентичности по seed-фразе (BIP39).

use p2pchat_ffi::ChatCore;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-seed-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.join("chat.db").to_str().unwrap().to_string()
}

#[test]
fn seed_roundtrip_restores_identity() {
    let a = ChatCore::new(tmp("a")).unwrap();
    let seed = a.export_seed().unwrap();
    assert_eq!(seed.split_whitespace().count(), 24, "24 слова");
    let pid = a.peer_id_hex();
    let fp = a.fingerprint();
    assert!(!fp.is_empty());

    let b = ChatCore::new(tmp("b")).unwrap();
    assert_ne!(b.peer_id_hex(), pid, "новое устройство — другая личность");

    let restored = b.import_seed(seed.clone()).unwrap();
    assert_eq!(restored, pid);
    assert_eq!(b.peer_id_hex(), pid);
    assert_eq!(b.fingerprint(), fp, "отпечаток совпадает");
    assert_eq!(b.export_seed().unwrap(), seed);
}

#[test]
fn import_blocked_when_groups_exist() {
    let a = ChatCore::new(tmp("c")).unwrap();
    let seed = a.export_seed().unwrap();

    let b = ChatCore::new(tmp("d")).unwrap();
    let _ = b.create_named_group("Group".into()).unwrap();
    assert!(
        b.import_seed(seed).is_err(),
        "нельзя затереть непустой профиль"
    );
}

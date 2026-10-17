//! Персистентность: личность, группы (MLS) и сообщения переживают перезапуск.

use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;
use p2pchat_storage::Store;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-test-{}-{}.db", std::process::id(), name));
    let _ = std::fs::remove_file(&p);
    p.to_str().unwrap().to_string()
}

#[test]
fn identity_group_and_messages_persist() {
    let path = tmp("persist");

    let (peer_id, public_key) = {
        let store = Store::open(&path).unwrap();
        let identity = Identity::generate();
        store.set_meta("identity", &identity.secret_bytes()).unwrap();
        let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
        e.load_state().unwrap();
        e.load_mls(&path).unwrap();

        let g = b"team".to_vec();
        e.create_group(g.clone()).unwrap();
        let _env = e.post(g.clone(), "привет".into()).unwrap();
        assert_eq!(e.log().ordered(&g).len(), 1);

        (e.peer_id(), e.public_key())
    };

    let store = Store::open(&path).unwrap();
    let secret = store.get_meta("identity").unwrap().unwrap();
    let identity = Identity::from_secret_bytes(&secret).unwrap();
    let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
    e.load_state().unwrap();
    e.load_mls(&path).unwrap();

    assert_eq!(e.peer_id(), peer_id, "тот же peer_id");
    assert_eq!(e.public_key(), public_key, "тот же публичный ключ");
    assert!(e.has_group(b"team"), "группа сохранилась");
    assert_eq!(e.groups(), vec![b"team".to_vec()]);
    assert_eq!(e.log().ordered(b"team").len(), 1, "сообщение сохранилось");
    assert_eq!(e.log().ordered(b"team")[0].text, "привет");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn read_marker_persists_and_unread_survives() {
    let path = tmp("read");
    let g = b"team".to_vec();

    {
        let store = Store::open(&path).unwrap();
        let identity = Identity::generate();
        store.set_meta("identity", &identity.secret_bytes()).unwrap();
        let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
        e.load_state().unwrap();
        e.create_group(g.clone()).unwrap();
        e.post(g.clone(), "m1".into()).unwrap();
        e.mark_read(&g).unwrap();
        e.post(g.clone(), "m2".into()).unwrap();
        assert_eq!(e.unread_count(&g).unwrap(), 1);
    }

    let store = Store::open(&path).unwrap();
    let identity =
        Identity::from_secret_bytes(&store.get_meta("identity").unwrap().unwrap()).unwrap();
    let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
    e.load_state().unwrap();
    assert_eq!(e.unread_count(&g).unwrap(), 1, "отметка прочтения сохранилась");

    let _ = std::fs::remove_file(&path);
}

#[test]
fn nickname_persists() {
    let path = tmp("nick");
    {
        let store = Store::open(&path).unwrap();
        let identity = Identity::generate();
        store.set_meta("identity", &identity.secret_bytes()).unwrap();
        let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
        e.load_state().unwrap();
        e.set_nickname("Дмитрий").unwrap();
    }
    let store = Store::open(&path).unwrap();
    let identity =
        Identity::from_secret_bytes(&store.get_meta("identity").unwrap().unwrap()).unwrap();
    let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
    e.load_state().unwrap();
    assert_eq!(e.my_nickname().as_deref(), Some("Дмитрий"));
    let _ = std::fs::remove_file(&path);
}

//! Вложения-гифки: контент-адресация, персистентность и выборки по группе/автору.

use p2pchat_core::proto::{Attachment, ChatMessage};
use p2pchat_core::{message_id, GossipEngine, Log};
use p2pchat_crypto::Identity;
use p2pchat_storage::Store;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-gifs-{}-{}.db", std::process::id(), name));
    let _ = std::fs::remove_file(&p);
    p.to_str().unwrap().to_string()
}

fn att(id: &str) -> Attachment {
    Attachment {
        kind: "klipy".into(),
        id: id.into(),
        width: 220,
        height: 230,
        url: String::new(),
    }
}

fn base_msg() -> ChatMessage {
    ChatMessage {
        id: Vec::new(),
        group_id: b"g".to_vec(),
        author: b"a".to_vec(),
        lamport: 1,
        wall_time: 2,
        text: "hi".into(),
        parents: Vec::new(),
        attachment: None,
    }
}

#[test]
fn message_id_changes_only_with_attachment() {
    let plain = base_msg();
    let id_plain = message_id(&plain);

    let mut with = base_msg();
    with.attachment = Some(att("g1"));
    let id_with = message_id(&with);

    assert_ne!(id_plain, id_with, "вложение меняет контент-адрес");
    assert_eq!(id_plain, message_id(&base_msg()), "детерминизм и стабильность");
    // другое вложение — другой id
    let mut other = base_msg();
    other.attachment = Some(att("g2"));
    assert_ne!(id_with, message_id(&other));
}

#[test]
fn gif_persists_and_is_queryable() {
    let path = tmp("persist");
    let g = b"team".to_vec();

    let peer_id = {
        let store = Store::open(&path).unwrap();
        let identity = Identity::generate();
        store.set_meta("identity", &identity.secret_bytes()).unwrap();
        let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
        e.load_state().unwrap();
        e.create_group(g.clone()).unwrap();
        e.post_gif(g.clone(), att("g1")).unwrap();

        assert_eq!(e.log().gifs_for_group(&g, 50).unwrap().len(), 1, "выборка по группе");
        assert_eq!(e.log().gifs_by_author(&e.peer_id(), 50).unwrap().len(), 1, "выборка «Мои»");
        e.peer_id()
    };

    // перезапуск: вложение на месте
    let store = Store::open(&path).unwrap();
    let secret = store.get_meta("identity").unwrap().unwrap();
    let identity = Identity::from_secret_bytes(&secret).unwrap();
    let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
    e.load_state().unwrap();

    assert_eq!(e.peer_id(), peer_id);
    let gifs = e.log().gifs_for_group(&g, 50).unwrap();
    assert_eq!(gifs.len(), 1);
    assert_eq!(gifs[0].kind, "klipy");
    assert_eq!(gifs[0].id, "g1");
    assert_eq!((gifs[0].width, gifs[0].height), (220, 230));

    // само сообщение тоже несёт вложение
    let msg = &e.log().ordered(&g)[0];
    assert_eq!(msg.attachment.as_ref().unwrap().id, "g1");
}

#[test]
fn gif_replicates_between_logs() {
    let g = b"team".to_vec();
    let mut a = Log::new();
    let mut b = Log::new();

    let msg = a
        .append_local(g.clone(), b"alice".to_vec(), String::new(), Some(att("g7")))
        .unwrap();
    assert!(b.merge(msg.clone()).unwrap());

    let got = b.get(&msg.id).expect("сообщение слито");
    assert_eq!(got.attachment.as_ref().unwrap().id, "g7");
    // повторный merge идемпотентен
    assert!(!b.merge(msg).unwrap());
}

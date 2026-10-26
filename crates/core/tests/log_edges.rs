//! Полное покрытие Log: память, SQLite-обёртки, топосортировка, gif-вложения.

use p2pchat_core::proto::{Attachment, ChatMessage, MembershipEvent};
use p2pchat_core::{message_id, Log};
use p2pchat_storage::Store;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-log-{}-{}.db", std::process::id(), name));
    let _ = std::fs::remove_file(&p);
    p.to_str().unwrap().to_string()
}

fn msg(
    group: &[u8],
    author: &[u8],
    text: &str,
    lamport: u64,
    parents: Vec<Vec<u8>>,
) -> ChatMessage {
    let mut m = ChatMessage {
        id: Vec::new(),
        group_id: group.to_vec(),
        author: author.to_vec(),
        lamport,
        wall_time: lamport,
        text: text.into(),
        parents,
        attachment: None,
    };
    m.id = message_id(&m);
    m
}

#[test]
fn memory_log_everything() {
    let mut l = Log::new();
    assert!(l.is_empty());
    assert_eq!(l.len(), 0);
    assert_eq!(l.lamport(), 0);
    assert!(l.heads(b"g").is_empty());
    assert!(l.get(b"nope").is_none());

    let a = l
        .append_local(
            b"g".to_vec(),
            b"alice".to_vec(),
            "hello".into(),
            Some(Attachment {
                kind: "gif".into(),
                id: "x".into(),
                width: 10,
                height: 20,
                url: "u".into(),
                mime: String::new(),
                size: 0,
                duration_ms: 0,
                file_name: String::new(),
                thumbnail: Vec::new(),
            }),
        )
        .unwrap();
    assert_eq!(l.len(), 1);
    assert!(!l.is_empty());
    assert_eq!(l.heads(b"g"), vec![a.id.clone()]);
    assert_eq!(l.ids_for_group(b"g"), vec![a.id.clone()]);
    assert!(l.missing_parents(b"g").is_empty());

    // повторное слияние — не новое
    assert!(!l.merge(a.clone()).unwrap());

    // сообщение с неизвестным родителем
    let b = msg(b"g", b"bob", "remote", 2, vec![b"missing-parent".to_vec()]);
    assert!(l.merge(b.clone()).unwrap());
    assert_eq!(l.missing_parents(b"g"), vec![b"missing-parent".to_vec()]);
    assert!(l.heads(b"g").contains(&b.id));
    let t = l.tick();
    assert_eq!(l.lamport(), t);
    assert!(t >= 2);

    // мета в памяти
    l.set_meta("k", b"v").unwrap();
    assert_eq!(l.get_meta("k").unwrap().unwrap(), b"v");
    l.set_read(b"g", b"mid").unwrap();
    assert_eq!(l.read_marker(b"g").unwrap().unwrap(), b"mid");

    // обёртки без Store — пустые/no-op
    assert!(l.load_profiles().unwrap().is_empty());
    assert!(l.load_edits().unwrap().is_empty());
    assert!(l.load_deletes().unwrap().is_empty());
    assert!(l.load_group_keys().unwrap().is_empty());
    assert!(l.load_membership().unwrap().is_empty());
    assert!(l.load_reactions().unwrap().is_empty());
    assert!(l.gifs_for_group(b"g", 10).unwrap().is_empty());
    assert!(l.gifs_by_author(b"alice", 10).unwrap().is_empty());
    l.store_group_key(b"g", b"k").unwrap();
    l.store_profile(b"p", "Ann", 1, None).unwrap();
    l.store_edit(b"m", "e", 1, 1).unwrap();
    l.store_delete(b"d").unwrap();
    l.store_reaction(b"m", b"p", "👍", 1).unwrap();
    l.remove_reaction(b"m", b"p", "👍").unwrap();
    l.store_membership(&MembershipEvent {
        group_id: b"g".to_vec(),
        peer_id: b"p".to_vec(),
        public_key: b"pk".to_vec(),
        join: true,
        lamport: 1,
        author: b"a".to_vec(),
        id: b"e".to_vec(),
    })
    .unwrap();

    // порядок: родитель раньше потомка даже при обратном добавлении
    let mut l2 = Log::new();
    let child = msg(b"g", b"a", "child", 2, vec![b"parent".to_vec()]);
    l2.merge(child.clone()).unwrap();
    let parent = ChatMessage {
        id: b"parent".to_vec(),
        group_id: b"g".to_vec(),
        author: b"a".to_vec(),
        lamport: 1,
        wall_time: 1,
        text: "parent".into(),
        parents: vec![],
        attachment: None,
    };
    l2.merge(parent.clone()).unwrap();
    let ordered = l2.ordered(b"g");
    assert_eq!(ordered[0].id, parent.id);
    assert_eq!(ordered[1].id, child.id);
    assert_eq!(Log::default().len(), 0);
}

#[test]
fn store_wrappers_and_recovery() {
    let path = tmp("all");
    let store = Store::open(&path).unwrap();
    let mut l = Log::with_store(store).unwrap();

    let m = l
        .append_local(b"g".to_vec(), b"alice".to_vec(), "hi".into(), None)
        .unwrap();
    l.store_group_key(b"g", b"key").unwrap();
    l.set_meta("custom", b"value").unwrap();
    l.set_read(b"g", &m.id).unwrap();
    l.store_profile(b"alice", "Alice", 2, Some(&[1, 2, 3]))
        .unwrap();
    l.store_edit(&m.id, "edited", 3, 4).unwrap();
    l.store_delete(&m.id).unwrap();
    l.store_reaction(&m.id, b"alice", "👍", 5).unwrap();
    l.store_reaction(&m.id, b"bob", "🔥", 6).unwrap();
    l.remove_reaction(&m.id, b"bob", "🔥").unwrap();
    l.store_membership(&MembershipEvent {
        group_id: b"g".to_vec(),
        peer_id: b"alice".to_vec(),
        public_key: b"pk".to_vec(),
        join: true,
        lamport: 1,
        author: b"alice".to_vec(),
        id: b"ev".to_vec(),
    })
    .unwrap();
    let gif = l
        .append_local(
            b"g".to_vec(),
            b"alice".to_vec(),
            String::new(),
            Some(Attachment {
                kind: "gif".into(),
                id: "g1".into(),
                width: 1,
                height: 2,
                url: String::new(),
                mime: String::new(),
                size: 0,
                duration_ms: 0,
                file_name: String::new(),
                thumbnail: Vec::new(),
            }),
        )
        .unwrap();

    assert_eq!(l.load_group_keys().unwrap().len(), 1);
    assert_eq!(l.get_meta("custom").unwrap().unwrap(), b"value");
    assert_eq!(l.read_marker(b"g").unwrap().unwrap(), m.id);
    assert_eq!(l.load_profiles().unwrap().len(), 1);
    assert_eq!(l.load_edits().unwrap().len(), 1);
    assert_eq!(l.load_deletes().unwrap().len(), 1);
    assert_eq!(l.load_membership().unwrap().len(), 1);
    assert_eq!(l.load_reactions().unwrap().len(), 1);
    assert_eq!(l.gifs_for_group(b"g", 10).unwrap().len(), 1);
    assert_eq!(l.gifs_by_author(b"alice", 10).unwrap().len(), 1);
    assert_eq!(l.gifs_for_group(b"other", 10).unwrap().len(), 0);
    let _ = gif;
    drop(l);

    // Повторное открытие: сообщения, lamport и мета восстановлены.
    let store = Store::open(&path).unwrap();
    let l = Log::with_store(store).unwrap();
    assert_eq!(l.len(), 2);
    assert!(l.lamport() >= 1);
    assert_eq!(l.get_meta("custom").unwrap().unwrap(), b"value");
    assert_eq!(l.ordered(b"g").len(), 2);
    let _ = std::fs::remove_file(&path);
}

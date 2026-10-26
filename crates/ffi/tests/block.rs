//! Блокировка на FFI-границе: история скрывается/возвращается.

use p2pchat_ffi::ChatCore;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-block-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.join("chat.db").to_str().unwrap().to_string()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

#[test]
fn block_hides_history() {
    let a = ChatCore::new(tmp("a")).unwrap();
    let g = a.create_named_group("G".into()).unwrap();
    let _ = a.post(g.clone(), "visible".into()).unwrap();
    assert_eq!(a.history(g.clone()).len(), 1);

    let me = unhex(&a.peer_id_hex());
    a.block_peer(me.clone());
    assert_eq!(a.history(g.clone()).len(), 0, "скрыто");
    assert_eq!(a.blocked_peers(), vec![me.clone()]);

    a.unblock_peer(me);
    assert_eq!(a.history(g).len(), 1, "снова видно");
}

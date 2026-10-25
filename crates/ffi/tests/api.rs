//! Широкий smoke-тест публичного FFI-API (как вызывает Android).

use p2pchat_ffi::{ChatCore, FfiAttachment, FfiPeerLink};

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-api-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.join("chat.db").to_str().unwrap().to_string()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn ffi_api_smoke() {
    let a = ChatCore::new(tmp("a")).unwrap();
    assert!(!a.peer_id_hex().is_empty());
    assert_eq!(a.public_key().len(), 32);
    assert!(a.fingerprint().contains(' '));
    assert_eq!(a.export_seed().unwrap().split_whitespace().count(), 24);
    let seed = a.export_seed().unwrap();
    let a_pid = unhex(&a.peer_id_hex());

    // Группа
    let g = a.create_named_group("Team".into()).unwrap();
    assert_eq!(a.group_name(g.clone()), "Team");
    assert!(a.is_owner(g.clone()).unwrap());
    assert!(a.list_groups().contains(&g));

    // Сообщения
    let out = a.post(g.clone(), "hello".into()).unwrap();
    assert!(!out.envelope.is_empty());
    let h = a.history(g.clone());
    assert_eq!(h.len(), 1);
    let id = h[0].id.clone();

    let _ = a.edit_message(g.clone(), id.clone(), "edited".into()).unwrap();
    let h = a.history(g.clone());
    assert_eq!(h[0].text, "edited");
    assert!(h[0].edited);

    a.mark_read(g.clone()).unwrap();
    assert_eq!(a.unread_count(g.clone()).unwrap(), 0);

    let _ = a.delete_message(g.clone(), id).unwrap();
    assert!(a.history(g.clone())[0].deleted);

    let page = a.history_window(g.clone(), None, 10);
    assert_eq!(page.messages.len(), 1);
    let page0 = a.history_window(g.clone(), None, 0);
    assert!(page0.messages.is_empty());

    // Профиль
    let _ = a.set_nickname("Ann".into()).unwrap();
    assert_eq!(a.my_nickname().as_deref(), Some("Ann"));
    assert_eq!(a.nickname_of(a_pid.clone()).as_deref(), Some("Ann"));
    assert_eq!(a.all_nicknames().len(), 1);
    assert!(a.profile_envelope().unwrap().is_some());

    let _ = a.set_avatar(vec![1, 2, 3]).unwrap();
    assert_eq!(a.avatar_of(a_pid.clone()), Some(vec![1, 2, 3]));
    assert_eq!(a.avatars().len(), 1);

    // Профиль группы
    let _ = a.set_group_avatar(g.clone(), vec![9, 9]).unwrap();
    assert_eq!(a.group_avatar(g.clone()), Some(vec![9, 9]));
    let _ = a.rename_group(g.clone(), "Renamed".into()).unwrap();
    assert_eq!(a.group_name(g.clone()), "Renamed");

    // Связи
    let _ = a
        .post_links(g.clone(), vec![FfiPeerLink { peer_id: a_pid.clone(), kind: "direct".into() }])
        .unwrap();
    assert_eq!(a.links(g.clone()).len(), 1);

    // Состав
    assert_eq!(a.members(g.clone()).len(), 1);
    assert!(a.neighbors(g.clone()).is_empty());
    assert!(a.neighbor_keys(g.clone()).is_empty());
    assert!(a.groups_with(a.public_key()).contains(&g));

    // Синхронизация/пиры
    let _ = a.sync_digest_to(g.clone(), a.public_key()).unwrap();
    assert!(a.join_envelope(g.clone()).unwrap().is_none());
    let _ = a.peers();

    // Гифки
    let att = FfiAttachment {
        kind: "gif".into(),
        id: "abc".into(),
        width: 10,
        height: 10,
        url: String::new(),
        mime: String::new(),
        size: 0,
        duration_ms: 0,
        file_name: String::new(),
        thumbnail: Vec::new(),
    };
    let _ = a.post_gif(g.clone(), att).unwrap();
    assert_eq!(a.recent_gifs(10).len(), 1);
    assert_eq!(a.group_gifs(g.clone(), 10).len(), 1);

    // Впуск второго узла
    let inv = a.group_invite(g.clone(), 1).unwrap().unwrap();
    let b = ChatCore::new(tmp("b")).unwrap();
    let join = b.join_from_invite(inv).unwrap();
    assert_eq!(join.group_id, g);
    let outs = a.receive(join.request.envelope).unwrap();
    assert!(!outs.is_empty());
    for o in outs {
        let _ = b.receive(o.envelope);
    }
    assert!(b.list_groups().contains(&g));
    assert_eq!(b.members(g.clone()).len(), 2);

    // Нельзя восстановить seed поверх непустого профиля
    assert!(b.import_seed(seed).is_err());

    // Удаление участника (владелец)
    let b_pid = unhex(&b.peer_id_hex());
    let rem = a.remove_member(g.clone(), b_pid).unwrap();
    assert!(!rem.is_empty());
    assert_eq!(a.members(g.clone()).len(), 1);

    // Выход
    b.leave_group(g.clone()).unwrap();
    assert!(!b.list_groups().contains(&g));
}

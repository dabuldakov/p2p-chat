//! Проверка впуска через FFI-границу (как это делает Android):
//! create_group_invite → join_from_invite → receive(request) → receive(welcome) → post.

use p2pchat_ffi::ChatCore;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-ffi-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.join("chat.db").to_str().unwrap().to_string()
}

#[test]
fn two_cores_join_and_message() {
    let a = ChatCore::new(tmp("a")).unwrap();
    let b = ChatCore::new(tmp("b")).unwrap();
    let g = b"team".to_vec();

    // A создаёт группу и билет
    let invite = a.create_group_invite(g.clone()).unwrap();
    assert!(invite.starts_with("p2pchat://join?g="));

    // B готовит заявку (KeyPackage + билет)
    let join = b.join_from_invite(invite).unwrap();
    assert_eq!(join.group_id, g);
    assert_eq!(join.host_key, a.public_key(), "заявка адресована хозяину");
    assert!(join.request.targets.contains(&a.public_key()));

    // A принимает заявку → Commit + Welcome
    let outs = a.receive(join.request.envelope.clone()).unwrap();
    assert!(!outs.is_empty(), "хозяин должен ответить (Commit+Welcome)");

    // B принимает ответ (Commit — до группы, Welcome — вход)
    for o in outs {
        let _ = b.receive(o.envelope).unwrap();
    }

    // B теперь в группе
    assert!(b.list_groups().contains(&g), "B вошёл в группу");
    assert_eq!(b.members(g.clone()).len(), 2);
    assert_eq!(a.members(g.clone()).len(), 2);

    // Обмен сообщениями
    let m = a.post(g.clone(), "привет".into()).unwrap();
    b.receive(m.envelope).unwrap();
    let page = b.history_window(g.clone(), None, 10);
    assert_eq!(page.messages.len(), 1);
    assert_eq!(page.messages[0].text, "привет");
}

#[test]
fn deleted_message_is_marked_as_tombstone() {
    let a = ChatCore::new(tmp("del")).unwrap();
    let g = a.create_named_group("G".into()).unwrap();
    let _ = a.post(g.clone(), "hi".into()).unwrap();

    let page = a.history_window(g.clone(), None, 10);
    let id = page.messages[0].id.clone();
    let _ = a.delete_message(g.clone(), id).unwrap();

    let page = a.history_window(g, None, 10);
    assert!(page.messages[0].deleted, "помечено удалённым");
    assert_eq!(page.messages[0].text, "", "текст скрыт");
}

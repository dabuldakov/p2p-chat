#![allow(clippy::needless_range_loop)]
//! Интеграционный сценарий: 10 участников в одной группе.
//!
//! Проверяем: обмен сообщениями от всех, правки/удаления, выход и повторный
//! вход, удаление владельцем, лимиты инвайтов, replay/tamper, изоляцию групп
//! и невозможность писать удалённому участнику.

mod common;

use std::collections::HashSet;

use common::*;
use p2pchat_core::GossipEngine;

fn texts(e: &GossipEngine, g: &[u8]) -> HashSet<String> {
    e.log().ordered(g).into_iter().map(|m| m.text).collect()
}

#[test]
fn ten_members_messages_edit_delete() {
    let (mut engines, keys, g) = build_group(10);

    for e in &engines {
        assert_eq!(e.members(&g).len(), 10, "в группе 10 участников");
    }

    // Каждый отправляет по сообщению — все должны увидеть все 10.
    let mut seed = Vec::new();
    for i in 0..10 {
        let out = engines[i].post(g.clone(), format!("m{i}")).unwrap();
        seed.push((i, out));
    }
    pump(&mut engines, &keys, seed);

    for e in &engines {
        let t = texts(e, &g);
        assert_eq!(t.len(), 10, "у всех 10 сообщений");
        for i in 0..10 {
            assert!(t.contains(&format!("m{i}")), "нет m{i}");
        }
    }

    // Правка автором — доезжает всем.
    let id0 = engines[0]
        .log()
        .ordered(&g)
        .into_iter()
        .find(|m| m.text == "m0")
        .unwrap()
        .id;
    let edit = engines[0]
        .post_edit(&g, &id0, "m0-edited".into())
        .unwrap()
        .unwrap();
    pump(&mut engines, &keys, vec![(0, edit)]);
    for e in &engines {
        let (text, edited) = e.effective_text(&id0, "m0");
        assert_eq!(text, "m0-edited");
        assert!(edited);
    }

    // Не-автор не может править/удалять.
    assert!(engines[1]
        .post_edit(&g, &id0, "hack".into())
        .unwrap()
        .is_none());
    assert!(engines[2].post_delete(&g, &id0).unwrap().is_none());

    // Удаление автором — tombstone у всех.
    let id1 = engines[1]
        .log()
        .ordered(&g)
        .into_iter()
        .find(|m| m.text == "m1")
        .unwrap()
        .id;
    let del = engines[1].post_delete(&g, &id1).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(1, del)]);
    for e in &engines {
        assert!(e.is_deleted(&id1), "удаление доехало");
    }
}

#[test]
fn remove_leave_rejoin() {
    let (mut engines, keys, g) = build_group(10);
    let p9 = engines[9].peer_id();

    // Владелец удаляет участника 9 (без его локального выхода).
    let commits = engines[0].remove_member(&g, &p9).unwrap();
    let seed: Vec<(usize, _)> = commits.into_iter().map(|o| (0, o)).collect();
    pump(&mut engines, &keys, seed);

    for i in 0..9 {
        assert_eq!(engines[i].members(&g).len(), 9, "у остальных 9 участников");
    }
    assert_eq!(
        engines[9].members(&g).len(),
        10,
        "у удалённого — устаревшее состояние"
    );

    // Новое сообщение владельца не адресуется удалённому и он не может его прочитать.
    let msg = engines[0].post(g.clone(), "after-remove".into()).unwrap();
    assert!(!msg.targets.contains(&engines[9].public_key()));
    let outcome = engines[9].handle(&enc(&msg));
    assert!(
        outcome.is_err() || outcome.unwrap().new_messages.is_empty(),
        "удалённый не читает новые сообщения"
    );

    // Сообщение удалённого не принимается оставшимися.
    let ghost = engines[9].post(g.clone(), "ghost".into()).unwrap();
    let outcome = engines[0].handle(&enc(&ghost));
    assert!(
        outcome.is_err() || outcome.unwrap().new_messages.is_empty(),
        "сообщение удалённого отклонено"
    );

    // Выход локально и повторный вход по новому инвайту.
    let _ = engines[9].leave_group(&g).unwrap();
    assert!(!engines[9].has_group(&g));

    let inv = engines[0].create_invite(&g, 3600, 1).unwrap();
    let (_gid, req) = engines[9].join_request(&inv).unwrap();
    pump(&mut engines, &keys, vec![(9, req)]);

    assert!(engines[9].has_group(&g), "повторно вошёл");
    for e in &engines {
        assert_eq!(e.members(&g).len(), 10, "снова 10 участников");
    }

    let out = engines[0].post(g.clone(), "after-rejoin".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    for e in &engines {
        assert!(
            texts(e, &g).contains("after-rejoin"),
            "сообщение после возврата"
        );
    }
}

#[test]
fn security_roles_replay_tamper_isolation() {
    let (mut engines, _keys, g) = build_group(10);

    // Не-владелец не может: приглашать, переименовывать, менять фото, удалять.
    assert!(engines[1].create_invite(&g, 60, 1).is_err());
    assert!(engines[1].set_group_name(&g, "x".into()).unwrap().is_none());
    assert!(engines[1].set_group_avatar(&g, vec![1]).unwrap().is_none());
    let p0 = engines[0].peer_id();
    assert!(engines[1].remove_member(&g, &p0).unwrap().is_empty());

    // Посторонний не получает адрес и не читает сообщения группы.
    let mut outsider = engine();
    let msg = engines[0].post(g.clone(), "secret".into()).unwrap();
    assert!(!msg.targets.contains(&outsider.public_key()));
    let outcome = outsider.handle(&enc(&msg));
    assert!(outcome.is_err() || outcome.unwrap().new_messages.is_empty());

    // Подмена байта конверта — отклоняется.
    let mut bad = enc(&msg);
    let n = bad.len();
    bad[n - 1] ^= 0xff;
    let outcome = engines[1].handle(&bad);
    assert!(
        outcome.is_err() || outcome.unwrap().new_messages.is_empty(),
        "tamper отклонён"
    );

    // Повтор того же конверта — replay отклоняется.
    let msg2 = engines[0].post(g.clone(), "replay".into()).unwrap();
    let bytes = enc(&msg2);
    let _ = engines[1].handle(&bytes).unwrap();
    assert!(engines[1].handle(&bytes).is_err(), "replay отклонён");

    // Инвайт с лимитом 2: третий вход не проходит.
    let mut host = engine();
    let g2 = host.create_named_group("Limit").unwrap();
    let inv = host.create_invite(&g2, 3600, 2).unwrap();
    let j1 = engine();
    let j2 = engine();
    let j3 = engine();
    let keys2 = vec![
        host.public_key(),
        j1.public_key(),
        j2.public_key(),
        j3.public_key(),
    ];
    let mut all = vec![host, j1, j2, j3];
    for j in 1..=2 {
        let (_, req) = all[j].join_request(&inv).unwrap();
        pump(&mut all, &keys2, vec![(j, req)]);
        assert!(all[j].has_group(&g2), "вход {j} разрешён");
    }
    let (_, req3) = all[3].join_request(&inv).unwrap();
    pump(&mut all, &keys2, vec![(3, req3)]);
    assert!(!all[3].has_group(&g2), "третий вход сверх лимита отклонён");

    // Просроченный инвайт не пускает.
    let mut h = engine();
    let g3 = h.create_named_group("Expired").unwrap();
    let inv3 = h.create_invite(&g3, 1, 1).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));
    let j = engine();
    let (_, req) = j.join_request(&inv3).unwrap();
    let keys3 = vec![h.public_key(), j.public_key()];
    let mut pair = vec![h, j];
    pump(&mut pair, &keys3, vec![(1, req)]);
    assert!(!pair[1].has_group(&g3), "просроченный инвайт отклонён");
}

#[test]
fn member_leave_notifies_group() {
    let (mut engines, keys, g) = build_group(5);

    let leave = engines[4].leave_group(&g).unwrap().expect("Leave-конверт");
    assert!(!engines[4].has_group(&g), "локально вышел сразу");

    pump(&mut engines, &keys, vec![(4, leave)]);

    for i in 0..4 {
        assert_eq!(engines[i].members(&g).len(), 4, "владелец удалил ушедшего");
    }

    // Сообщения продолжаются без ушедшего, ему больше не адресуются.
    let out = engines[0].post(g.clone(), "after-leave".into()).unwrap();
    assert!(!out.targets.contains(&engines[4].public_key()));
    pump(&mut engines, &keys, vec![(0, out)]);
    for i in 0..4 {
        assert!(texts(&engines[i], &g).contains("after-leave"));
    }

    // Ушедший не может читать новые сообщения (локального состояния нет).
    let msg = engines[0]
        .post(g.clone(), "secret-after-leave".into())
        .unwrap();
    let outcome = engines[4].handle(&enc(&msg));
    assert!(outcome.is_err() || outcome.unwrap().new_messages.is_empty());
}

#[test]
fn owner_leave_transfers_ownership() {
    let (mut engines, keys, g) = build_group(4);
    let owner = engines[0].peer_id();
    let new_owner = engines[0]
        .members(&g)
        .into_iter()
        .map(|m| m.peer_id)
        .filter(|p| p != &owner)
        .min()
        .unwrap();
    let new_owner_idx = engines
        .iter()
        .position(|e| e.peer_id() == new_owner)
        .unwrap();

    let leave = engines[0].leave_group(&g).unwrap().expect("Leave-конверт");
    pump(&mut engines, &keys, vec![(0, leave)]);

    for i in 1..4 {
        assert_eq!(engines[i].members(&g).len(), 3, "осталось 3 участника");
    }
    assert!(
        engines[new_owner_idx].is_owner(&g).unwrap(),
        "владение перешло"
    );
    for i in 1..4 {
        if i != new_owner_idx {
            assert!(!engines[i].is_owner(&g).unwrap());
        }
    }
    // Новый владелец может приглашать.
    assert!(engines[new_owner_idx].create_invite(&g, 60, 1).is_ok());
}

#[test]
fn owner_leave_single_member_transfers() {
    let (mut engines, keys, g) = build_group(2);

    let leave = engines[0].leave_group(&g).unwrap().expect("Leave-конверт");
    pump(&mut engines, &keys, vec![(0, leave)]);

    assert_eq!(engines[1].members(&g).len(), 1, "остался один");
    assert!(
        engines[1].is_owner(&g).unwrap(),
        "единственный стал владельцем"
    );
    assert!(engines[1].create_invite(&g, 60, 1).is_ok());
}

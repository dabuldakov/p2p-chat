#![allow(clippy::needless_range_loop)]
//! Большой стресс-сценарий: 30 пользователей, 100 групп, пересылка по оверлею,
//! 100 сообщений на пользователя, часть изменена/удалена, проверка консистентности.
//!
//! Группы по 6..10 участников → оверлей использует кольцо/FANOUT, поэтому
//! сообщения доходят до «дальних» узлов через пересылку, а не напрямую.

mod common;

use std::collections::HashSet;

use common::*;
use p2pchat_core::GossipEngine;

const USERS: usize = 30;
const GROUPS: usize = 100;
const MSGS_PER_USER: usize = 100;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() >> 33) as usize % n
    }
}

#[derive(PartialEq)]
enum Action {
    Keep,
    Edit,
    Delete,
}

struct Post {
    author: usize,
    gid: Vec<u8>,
    id: Vec<u8>,
    text: String,
    action: Action,
}

fn snapshot(e: &GossipEngine, ids: &[Vec<u8>], texts: &[String]) -> Vec<(Vec<u8>, String, bool, bool)> {
    ids.iter()
        .zip(texts)
        .map(|(id, base)| {
            let (text, edited) = e.effective_text(id, base);
            (id.clone(), text, edited, e.is_deleted(id))
        })
        .collect()
}

#[test]
#[ignore = "долгий (~9 мин): 100 групп × 30 пользователей; запуск: cargo test -p p2pchat-core --test stress100 -- --ignored"]
fn stress_100_groups_30_users() {
    let mut engines: Vec<GossipEngine> = (0..USERS).map(|_| engine()).collect();
    let keys: Vec<Vec<u8>> = engines.iter().map(|e| e.public_key()).collect();
    let mut rng = Lcg(20261022);

    // --- 100 групп; гарантируем, что каждый пользователь хотя бы в одной ---
    let mut groups: Vec<(Vec<u8>, Vec<usize>)> = Vec::new();
    for gi in 0..GROUPS {
        let size = 6 + rng.below(5); // 6..=10
        let mut members = vec![gi % USERS];
        while members.len() < size {
            let u = rng.below(USERS);
            if !members.contains(&u) {
                members.push(u);
            }
        }
        let owner = members[0];
        let gid = engines[owner].create_named_group(&format!("group-{gi}")).unwrap();

        for &m in &members[1..] {
            let inv = engines[owner].create_invite(&gid, 3600, 1).unwrap();
            let (jg, req) = engines[m].join_request(&inv).unwrap();
            assert_eq!(jg, gid);
            pump(&mut engines, &keys, vec![(m, req)]);
        }
        assert_eq!(engines[owner].members(&gid).len(), size, "группа {gi}");
        groups.push((gid, members));
    }

    // --- по 100 сообщений на пользователя, распределены по его группам ---
    let mut posts: Vec<Post> = Vec::new();
    let mut seed = Vec::new();
    for u in 0..USERS {
        let my: Vec<Vec<u8>> = groups
            .iter()
            .filter(|(_, m)| m.contains(&u))
            .map(|(g, _)| g.clone())
            .collect();
        assert!(!my.is_empty(), "пользователь {u} без групп");
        for i in 0..MSGS_PER_USER {
            let gid = my[i % my.len()].clone();
            let text = format!("u{u}-m{i}");
            let out = engines[u].post(gid.clone(), text.clone()).unwrap();
            let id = engines[u].log().ordered(&gid).last().unwrap().id.clone();
            let action = if i % 7 == 0 {
                Action::Delete
            } else if i % 5 == 0 {
                Action::Edit
            } else {
                Action::Keep
            };
            posts.push(Post { author: u, gid, id, text, action });
            seed.push((u, out));
        }
    }
    pump(&mut engines, &keys, seed);

    // --- часть изменить, часть удалить (только автор) ---
    let mut seed2 = Vec::new();
    for p in &posts {
        match p.action {
            Action::Edit => {
                if let Some(out) = engines[p.author]
                    .post_edit(&p.gid, &p.id, format!("{}-edited", p.text))
                    .unwrap()
                {
                    seed2.push((p.author, out));
                }
            }
            Action::Delete => {
                if let Some(out) = engines[p.author].post_delete(&p.gid, &p.id).unwrap() {
                    seed2.push((p.author, out));
                }
            }
            Action::Keep => {}
        }
    }
    pump(&mut engines, &keys, seed2);

    // Не-автор не может править/удалять (проверяем на случайном сообщении).
    let sample = &posts[0];
    let other = (0..USERS).find(|u| *u != sample.author).unwrap();
    if engines[other].has_group(&sample.gid) {
        assert!(engines[other]
            .post_edit(&sample.gid, &sample.id, "hack".into())
            .unwrap()
            .is_none());
        assert!(engines[other].post_delete(&sample.gid, &sample.id).unwrap().is_none());
    }

    // --- консистентность по каждой группе ---
    for (gid, members) in &groups {
        let owner = members[0];
        let ordered = engines[owner].log().ordered(gid);
        let ids: Vec<Vec<u8>> = ordered.iter().map(|m| m.id.clone()).collect();
        let texts: Vec<String> = ordered.iter().map(|m| m.text.clone()).collect();
        let expected = posts.iter().filter(|p| &p.gid == gid).count();
        assert_eq!(ids.len(), expected, "все посты группы доехали");
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len(), "без дублей");

        let owner_snap = snapshot(&engines[owner], &ids, &texts);
        for &u in members {
            assert!(engines[u].has_group(gid), "участник {u} в группе");
            assert_eq!(engines[u].members(gid).len(), members.len(), "состав группы {u}");
            let member_ids: Vec<Vec<u8>> = engines[u]
                .log()
                .ordered(gid)
                .iter()
                .map(|m| m.id.clone())
                .collect();
            assert_eq!(member_ids, ids, "у {u} тот же набор и порядок");
            assert_eq!(snapshot(&engines[u], &ids, &texts), owner_snap, "у {u} те же тексты/удаления");
        }

        // Посторонний не состоит в группе и не видит её.
        let outsider = (0..USERS).find(|u| !members.contains(u)).unwrap();
        assert!(!engines[outsider].has_group(gid));
        assert!(engines[outsider].members(gid).is_empty());
    }

    // --- выход и повторный вход в первой группе ---
    let (gid, members) = groups[0].clone();
    let leaver = members[1];
    let leave = engines[leaver].leave_group(&gid).unwrap().expect("Leave-конверт");
    pump(&mut engines, &keys, vec![(leaver, leave)]);
    for &u in &members {
        if u != leaver {
            assert_eq!(engines[u].members(&gid).len(), members.len() - 1, "ушедшего удалили");
        }
    }

    let inv = engines[members[0]].create_invite(&gid, 3600, 1).unwrap();
    let (_, req) = engines[leaver].join_request(&inv).unwrap();
    pump(&mut engines, &keys, vec![(leaver, req)]);
    for &u in &members {
        assert_eq!(engines[u].members(&gid).len(), members.len(), "вернулся");
    }
}

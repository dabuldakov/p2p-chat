//! Публичные операции GossipEngine, не входящие в основные сценарии.

mod common;

use common::*;
use p2pchat_core::proto::{Attachment, Gossip};
use prost::Message as _;

#[test]
fn profile_group_links_edit_delete() {
    let (mut engines, keys, g) = build_group(2);
    let p0 = engines[0].peer_id();
    let p1 = engines[1].peer_id();

    let out = engines[0].set_nickname("Owner").unwrap().unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    assert_eq!(engines[1].nickname_of(&p0).as_deref(), Some("Owner"));
    assert_eq!(engines[1].my_nickname().as_deref(), None);
    assert!(!engines[1].all_nicknames().is_empty());
    assert!(engines[0].profile_envelope().unwrap().is_some());

    let out = engines[0].set_avatar(vec![1, 2, 3]).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    assert_eq!(engines[1].avatar_of(&p0), Some(vec![1, 2, 3]));
    assert_eq!(engines[1].avatars().len(), 1);
    let out = engines[0].set_avatar(Vec::new()).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    assert!(engines[1].avatar_of(&p0).is_none());

    let out = engines[0].set_group_name(&g, "New".into()).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    assert_eq!(engines[1].group_name(&g), "New");
    assert!(engines[0].set_group_name(&g, "  ".into()).unwrap().is_none());
    assert!(engines[1].set_group_name(&g, "Hack".into()).unwrap().is_none());

    let out = engines[0]
        .set_group_avatar(&g, vec![7, 8])
        .unwrap()
        .unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    assert_eq!(engines[1].group_avatar(&g), Some(vec![7, 8]));
    assert!(engines[1].set_group_avatar(&g, vec![9]).unwrap().is_none());
    let out = engines[0]
        .set_group_avatar(&g, Vec::new())
        .unwrap()
        .unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    assert!(engines[1].group_avatar(&g).is_none());

    let out = engines[0]
        .post_links(&g, vec![(p1.clone(), "direct".into())])
        .unwrap()
        .unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    assert!(engines[1]
        .links(&g)
        .iter()
        .any(|(a, b, k)| a == &p0 && b == &p1 && k == "direct"));
    assert!(engines[0]
        .post_links(b"nope", vec![(p1.clone(), "x".into())])
        .unwrap()
        .is_none());
    assert!(engines[0].links(b"nope").is_empty());

    let out = engines[0].post(g.clone(), "m".into()).unwrap();
    pump(&mut engines, &keys, vec![(0, out)]);
    let id = engines[0].log().ordered(&g).last().unwrap().id.clone();
    let gif = engines[0]
        .post_gif(
            g.clone(),
            Attachment {
                kind: "gif".into(),
                id: "abc".into(),
                width: 2,
                height: 3,
                url: String::new(),
                mime: String::new(),
                size: 0,
                duration_ms: 0,
                file_name: String::new(),
                thumbnail: Vec::new(),
            },
        )
        .unwrap();
    pump(&mut engines, &keys, vec![(0, gif)]);

    let edit = engines[0]
        .post_edit(&g, &id, "edited".into())
        .unwrap()
        .unwrap();
    pump(&mut engines, &keys, vec![(0, edit)]);
    assert_eq!(engines[1].effective_text(&id, "m"), ("edited".into(), true));
    assert!(engines[1].post_edit(&g, &id, "hack".into()).unwrap().is_none());
    assert!(engines[0]
        .post_edit(&g, b"none", "x".into())
        .unwrap()
        .is_none());
    assert!(engines[0]
        .post_edit(b"nope", &id, "x".into())
        .unwrap()
        .is_none());

    let del = engines[0].post_delete(&g, &id).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(0, del)]);
    assert!(engines[1].is_deleted(&id));
    assert!(engines[1].post_delete(&g, &id).unwrap().is_none());
    assert!(engines[0].post_delete(&g, b"none").unwrap().is_none());

    engines[0].block_user(&p1);
    assert!(engines[0].blocked_users().contains(&p1));
    engines[0].unblock_user(&p1);
    assert!(!engines[0].blocked_users().contains(&p1));

    assert!(engines[0].groups_with(&engines[1].public_key()).contains(&g));
    assert!(engines[0].announce_peers().is_ok());
    assert!(engines[0].owner_alive(&g).unwrap().is_some());
    assert!(engines[1].owner_alive(&g).unwrap().is_none());
    assert!(engines[0].owner_last_seen(&g) > 0);
    assert!(engines[0].has_group(&g));
    assert_eq!(engines[0].groups(), vec![g.clone()]);
    assert_eq!(engines[0].neighbors(&g).len(), 1);
    assert!(!engines[0].neighbor_keys(&g).is_empty());
}

#[test]
fn invite_leave_remove_and_freeze() {
    let (mut engines, keys, g) = build_group(3);
    let p1 = engines[1].peer_id();
    let p2 = engines[2].peer_id();

    assert!(engines[1].create_invite(&g, 60, 1).is_err());
    assert!(engines[0].create_invite(b"nope", 60, 1).is_err());
    let invite = engines[0].create_invite(&g, 0, 0).unwrap();
    assert!(engines[1].join_request("bad").is_err());
    assert!(engines[1].join_request(&invite).is_ok());

    assert!(engines[1].remove_member(&g, &p2).unwrap().is_empty());
    assert!(engines[0].remove_member(&g, b"none").unwrap().is_empty());

    // Заморозка состава на время выборов.
    let claim = engines[1]
        .create_owner_claim(&g, p2pchat_core::proto::OwnerTrigger::Challenge)
        .unwrap()
        .unwrap();
    pump(&mut engines, &keys, vec![(1, claim)]);
    assert!(engines[0].create_invite(&g, 60, 1).is_err());
    assert!(engines[0].remove_member(&g, &p2).unwrap().is_empty());

    // Выход не-владельца.
    let leave = engines[2].leave_group(&g).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(2, leave)]);
    assert!(!engines[2].has_group(&g));
    assert!(engines[0].members(&g).len() <= 2);
    assert!(engines[2].leave_group(&g).unwrap().is_none());

    // Выход владельца передаёт роль.
    let leave = engines[0].leave_group(&g).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(0, leave)]);
    assert!(!engines[0].has_group(&g));
    assert!(engines[1].owner_of(&g).is_some());
    let owner = engines[1].owner_of(&g).unwrap();
    assert!(owner == p1 || owner == p2);
}

#[test]
fn create_named_group_and_replace_identity() {
    let mut e = engine();
    let g = e.create_named_group("Team").unwrap();
    assert_eq!(e.group_name(&g), "Team");
    let g2 = e.create_named_group("   ").unwrap();
    assert_eq!(e.group_name(&g2), String::from_utf8_lossy(&g2));
    assert!(e.group_name(b"legacy") == "legacy");

    let out = e.set_nickname("Old").unwrap().unwrap();
    assert!(matches!(
        Gossip::decode(out.envelope.payload.as_slice()).unwrap().body,
        Some(p2pchat_core::proto::gossip::Body::Profile(_))
    ));
    let id = p2pchat_crypto::Identity::generate();
    let new_pid = id.peer_id().to_vec();
    e.replace_identity(id).unwrap();
    assert_eq!(e.peer_id(), new_pid);
    assert!(e.my_nickname().is_none());
    assert!(e.log().get_meta("identity").unwrap().is_some());
}

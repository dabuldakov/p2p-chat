//! Выборы владельца: отсутствие, вызов, авто-преемник, возврат владельца.

mod common;

use common::*;
use p2pchat_core::gossip::Outgoing;
use p2pchat_core::proto::gossip::Body;
use p2pchat_core::proto::{Gossip, OwnerTrigger, PeerInfo};
use prost::Message as _;

const DAY: u64 = 24 * 3600;

fn set_now(engines: &mut [p2pchat_core::GossipEngine], t: u64) {
    for e in engines {
        e.set_test_now(Some(t));
    }
}

fn claim_id(out: &Outgoing) -> Vec<u8> {
    let g = Gossip::decode(out.envelope.payload.as_slice()).unwrap();
    match g.body {
        Some(Body::OwnerClaim(c)) => c.claim_id,
        _ => panic!("not an owner claim"),
    }
}

fn mark_active(engines: &mut [p2pchat_core::GossipEngine], keys: &[Vec<u8>], t: u64) {
    for e in engines.iter_mut() {
        for k in keys {
            e.peers_mut().upsert(PeerInfo {
                peer_id: p2pchat_crypto::content_id(k).to_vec(),
                endpoints: Vec::new(),
                last_seen: t,
                is_relay: false,
                hops: 1,
            });
        }
    }
}

fn is_owner(e: &p2pchat_core::GossipEngine, g: &[u8], who: &[u8]) -> bool {
    e.owner_of(g).as_deref() == Some(who)
}

#[test]
fn absence_election_transfers_ownership() {
    let (mut engines, keys, g) = build_group(3);
    let owner = engines[0].peer_id();
    let challenger = engines[1].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);

    let out = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("заявка при отсутствии владельца");
    let cid = claim_id(&out);
    pump(&mut engines, &keys, vec![(1, out)]);
    assert!(
        is_owner(&engines[0], &g, &owner),
        "до голосования владелец прежний"
    );

    let vote_t = t + DAY + 3600;
    set_now(&mut engines, vote_t);
    let vote = engines[2]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("голос за кандидата");
    pump(&mut engines, &keys, vec![(2, vote)]);

    let end_t = t + 2 * DAY + 60;
    set_now(&mut engines, end_t);
    let change = engines[1]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("выборы завершены");
    pump(&mut engines, &keys, vec![(1, change)]);

    for e in &engines {
        assert!(is_owner(e, &g, &challenger), "новый владелец принят всеми");
        assert_eq!(e.owner_epoch(&g), 1);
    }
}

#[test]
fn challenge_needs_majority_of_all_members() {
    let (mut engines, keys, g) = build_group(4);
    let owner = engines[0].peer_id();
    let challenger = engines[1].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 3600;
    set_now(&mut engines, t);

    let out = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .expect("вызов владельцу");
    let cid = claim_id(&out);
    pump(&mut engines, &keys, vec![(1, out)]);

    let vote_t = t + DAY + 3600;
    set_now(&mut engines, vote_t);
    let v2 = engines[2]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("v2");
    let v3 = engines[3]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("v3");
    let keep = engines[0]
        .owner_vote(&g, &[], true, true)
        .unwrap()
        .expect("keep");
    pump(&mut engines, &keys, vec![(2, v2), (3, v3), (0, keep)]);

    let end_t = t + 2 * DAY + 60;
    set_now(&mut engines, end_t);
    let change = engines[1]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("вызов удался: 3 из 4");
    pump(&mut engines, &keys, vec![(1, change)]);

    for e in &engines {
        assert!(
            is_owner(e, &g, &challenger),
            "вызов большинством сменил владельца"
        );
        assert_eq!(e.owner_epoch(&g), 1);
    }
    let _ = owner;
}

#[test]
fn challenge_without_majority_keeps_owner() {
    let (mut engines, keys, g) = build_group(4);
    let owner = engines[0].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 3600;
    set_now(&mut engines, t);

    let out = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .expect("вызов");
    let cid = claim_id(&out);
    pump(&mut engines, &keys, vec![(1, out)]);

    let vote_t = t + DAY + 3600;
    set_now(&mut engines, vote_t);
    let v2 = engines[2]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("v2");
    let keep = engines[0]
        .owner_vote(&g, &[], true, true)
        .unwrap()
        .expect("keep");
    pump(&mut engines, &keys, vec![(2, v2), (0, keep)]);

    let end_t = t + 2 * DAY + 60;
    set_now(&mut engines, end_t);
    assert!(
        engines[1].finalize_owner_election(&g).unwrap().is_none(),
        "2 из 4 недостаточно"
    );
    for e in &engines {
        assert!(is_owner(e, &g, &owner), "владелец остался");
        assert_eq!(e.owner_epoch(&g), 0);
    }
}

#[test]
fn auto_candidate_starts_after_application_window() {
    let (mut engines, keys, g) = build_group(3);
    let owner = engines[0].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + DAY + 60;
    set_now(&mut engines, t);
    mark_active(&mut engines, &keys, t);
    let successor = engines[1].peer_id().min(engines[2].peer_id());
    let creator = if engines[1].peer_id() == successor {
        1
    } else {
        2
    };
    let voter = if creator == 1 { 2 } else { 1 };

    let out = engines[creator]
        .create_owner_claim(&g, OwnerTrigger::Auto)
        .unwrap()
        .expect("авто-кандидат — минимальный peer_id");
    let cid = claim_id(&out);
    pump(&mut engines, &keys, vec![(creator, out)]);

    set_now(&mut engines, t + 3600);
    let vote = engines[voter]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("голос");
    pump(&mut engines, &keys, vec![(voter, vote)]);

    set_now(&mut engines, t + DAY + 60);
    let change = engines[creator]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("авто-выборы завершены");
    pump(&mut engines, &keys, vec![(creator, change)]);

    for e in &engines {
        assert!(is_owner(e, &g, &successor));
        assert_eq!(e.owner_epoch(&g), 1);
    }
    let _ = owner;
}

#[test]
fn owner_return_cancels_absence_election() {
    let (mut engines, keys, g) = build_group(3);
    let owner = engines[0].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);

    let out = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("заявка");
    let cid = claim_id(&out);
    pump(&mut engines, &keys, vec![(1, out)]);

    set_now(&mut engines, t + DAY + 3600);
    let vote = engines[2]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("голос");
    pump(&mut engines, &keys, vec![(2, vote)]);

    set_now(&mut engines, t + DAY + 7200);
    let alive = engines[0]
        .owner_alive(&g)
        .unwrap()
        .expect("владелец вернулся");
    pump(&mut engines, &keys, vec![(0, alive)]);

    set_now(&mut engines, t + 2 * DAY + 60);
    assert!(
        engines[1].finalize_owner_election(&g).unwrap().is_none(),
        "возврат владельца отменяет выборы"
    );
    for e in &engines {
        assert!(is_owner(e, &g, &owner));
        assert_eq!(e.owner_epoch(&g), 0);
    }
}

#[test]
fn rejection_and_cooldown_block_absence_takeover() {
    let (mut engines, keys, g) = build_group(3);
    let owner = engines[0].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);

    let out = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("заявка");
    let cid = claim_id(&out);
    pump(&mut engines, &keys, vec![(1, out)]);

    set_now(&mut engines, t + DAY + 3600);
    let vote = engines[2]
        .owner_vote(&g, &cid, false, false)
        .unwrap()
        .expect("голос против");
    pump(&mut engines, &keys, vec![(2, vote)]);

    let end_t = t + 2 * DAY + 60;
    set_now(&mut engines, end_t);
    assert!(
        engines[1].finalize_owner_election(&g).unwrap().is_none(),
        "1 за / 1 против — большинства нет"
    );
    assert!(is_owner(&engines[1], &g, &owner));
    assert!(
        engines[1]
            .create_owner_claim(&g, OwnerTrigger::Absent)
            .unwrap()
            .is_none(),
        "после провала — cooldown 3 дня"
    );
}

#[test]
fn active_election_survives_restart() {
    use p2pchat_core::{GossipEngine, Log};
    use p2pchat_crypto::Identity;
    use p2pchat_storage::Store;

    fn tmp(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "p2pchat-election-{}-{}.db",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_file(&p);
        p.to_str().unwrap().to_string()
    }
    fn open(path: &str) -> GossipEngine {
        let store = Store::open(path).unwrap();
        let identity = match store.get_meta("identity").unwrap() {
            Some(secret) => Identity::from_secret_bytes(&secret).unwrap(),
            None => {
                let id = Identity::generate();
                store.set_meta("identity", &id.secret_bytes()).unwrap();
                id
            }
        };
        let mut e = GossipEngine::new(identity, Log::with_store(store).unwrap());
        e.load_state().unwrap();
        e.load_mls(path).unwrap();
        e
    }

    let host_path = tmp("host");
    let join_path = tmp("join");
    let g = b"team".to_vec();
    let t = {
        let mut host = open(&host_path);
        host.create_group(g.clone()).unwrap();
        let invite = host.create_invite(&g, 3600, 1).unwrap();
        let joiner = open(&join_path);
        let (_gid, req) = joiner.join_request(&invite).unwrap();
        let keys = vec![host.public_key(), joiner.public_key()];
        pump(&mut [host, joiner], &keys, vec![(1, req)]);
        let mut joiner = open(&join_path);
        let last = joiner.owner_last_seen(&g);
        let t = last + 7 * DAY + 60;
        joiner.set_test_now(Some(t));
        joiner
            .create_owner_claim(&g, OwnerTrigger::Absent)
            .unwrap()
            .expect("заявка");
        t
    };

    let joiner = open(&join_path);
    let e = joiner.owner_election(&g).expect("выборы восстановлены");
    assert_eq!(e.claims.len(), 1);
    assert_eq!(e.created_at, t);
    assert_eq!(e.trigger, OwnerTrigger::Absent as i32);
    let _ = std::fs::remove_file(&host_path);
    let _ = std::fs::remove_file(&join_path);
}

#[test]
fn auto_skips_inactive_successor() {
    let (mut engines, keys, g) = build_group(4);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + DAY + 60;
    set_now(&mut engines, t);

    let mut nonowners: Vec<usize> = (1..4).collect();
    nonowners.sort_by(|a, b| engines[*a].peer_id().cmp(&engines[*b].peer_id()));
    let _first = nonowners[0];
    let second = nonowners[1];
    let second_id = engines[second].peer_id();
    // Первый наследник «неактивен» (нет свежего last_seen у остальных).
    for e in engines.iter_mut() {
        e.peers_mut().upsert(PeerInfo {
            peer_id: second_id.clone(),
            endpoints: Vec::new(),
            last_seen: t,
            is_relay: false,
            hops: 1,
        });
    }
    let out = engines[second]
        .create_owner_claim(&g, OwnerTrigger::Auto)
        .unwrap()
        .expect("второй активный наследник");
    let cid = claim_id(&out);
    pump(&mut engines, &keys, vec![(second, out)]);

    set_now(&mut engines, t + 3600);
    let voter = (1..4).find(|i| *i != second).unwrap();
    let vote = engines[voter]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("голос");
    pump(&mut engines, &keys, vec![(voter, vote)]);

    set_now(&mut engines, t + DAY + 60);
    let change = engines[second]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("авто-выборы");
    pump(&mut engines, &keys, vec![(second, change)]);
    for e in &engines {
        assert!(is_owner(e, &g, &engines[second].peer_id()));
    }
}

#[test]
fn auto_wins_without_votes_after_confirmation_window() {
    let (mut engines, keys, g) = build_group(3);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + DAY + 60;
    set_now(&mut engines, t);
    mark_active(&mut engines, &keys, t);
    let mut nonowners: Vec<usize> = (1..3).collect();
    nonowners.sort_by(|a, b| engines[*a].peer_id().cmp(&engines[*b].peer_id()));
    let creator = nonowners[0];
    let successor = engines[creator].peer_id();

    let out = engines[creator]
        .create_owner_claim(&g, OwnerTrigger::Auto)
        .unwrap()
        .expect("авто-заявка");
    pump(&mut engines, &keys, vec![(creator, out)]);

    set_now(&mut engines, t + DAY + 60);
    let change = engines[creator]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("молчаливое подтверждение");
    pump(&mut engines, &keys, vec![(creator, change)]);
    for e in &engines {
        assert!(is_owner(e, &g, &successor));
    }
}

#[test]
fn auto_objections_cancel_takeover() {
    let (mut engines, keys, g) = build_group(4);
    let owner = engines[0].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + DAY + 60;
    set_now(&mut engines, t);
    mark_active(&mut engines, &keys, t);
    let mut nonowners: Vec<usize> = (1..4).collect();
    nonowners.sort_by(|a, b| engines[*a].peer_id().cmp(&engines[*b].peer_id()));
    let creator = nonowners[0];
    let out = engines[creator]
        .create_owner_claim(&g, OwnerTrigger::Auto)
        .unwrap()
        .expect("авто-заявка");
    let cid = claim_id(&out);
    pump(&mut engines, &keys, vec![(creator, out)]);

    set_now(&mut engines, t + 3600);
    let mut votes = Vec::new();
    for i in nonowners.iter().copied().filter(|i| *i != creator) {
        votes.push((
            i,
            engines[i]
                .owner_vote(&g, &cid, false, false)
                .unwrap()
                .expect("возражение"),
        ));
    }
    pump(&mut engines, &keys, votes);

    set_now(&mut engines, t + DAY + 60);
    assert!(
        engines[creator]
            .finalize_owner_election(&g)
            .unwrap()
            .is_none(),
        "2 возражения из 3 отменяют авто-преемника"
    );
    for e in &engines {
        assert!(is_owner(e, &g, &owner));
    }
}

#[test]
fn multiple_claims_choose_relative_majority() {
    let (mut engines, keys, g) = build_group(4);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);

    let c1 = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("первая заявка");
    let c1_id = claim_id(&c1);
    pump(&mut engines, &keys, vec![(1, c1)]);

    let c2 = engines[2]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("вторая заявка в том же окне");
    pump(&mut engines, &keys, vec![(2, c2)]);
    assert_eq!(engines[3].owner_election(&g).unwrap().claims.len(), 2);

    set_now(&mut engines, t + DAY + 3600);
    let vote = engines[3]
        .owner_vote(&g, &c1_id, true, false)
        .unwrap()
        .expect("голос за первую");
    pump(&mut engines, &keys, vec![(3, vote)]);

    set_now(&mut engines, t + 2 * DAY + 60);
    let change = engines[3]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("есть большинство");
    let gos = Gossip::decode(change.envelope.payload.as_slice()).unwrap();
    match gos.body {
        Some(Body::OwnerChange(ch)) => assert_eq!(ch.candidates.len(), 2),
        _ => panic!("not an owner change"),
    }
    pump(&mut engines, &keys, vec![(3, change)]);

    let winner = engines[1].peer_id();
    for e in &engines {
        assert!(is_owner(e, &g, &winner), "победила заявка с 2 голосами");
    }
}

#[test]
fn owner_change_replays_to_offline_member() {
    let (mut engines, keys, g) = build_group(4);
    let owner = engines[0].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);

    let claim = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("заявка");
    let cid = claim_id(&claim);
    let claim_bytes = enc(&claim);
    for i in [0usize, 2] {
        engines[i].handle(&claim_bytes).unwrap();
    }

    set_now(&mut engines, t + DAY + 3600);
    let vote = engines[2]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("голос");
    let vote_bytes = enc(&vote);
    for i in [0usize, 1] {
        engines[i].handle(&vote_bytes).unwrap();
    }

    set_now(&mut engines, t + 2 * DAY + 60);
    let change = engines[1]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("смена владельца");
    let change_bytes = enc(&change);
    for i in [0usize, 2] {
        engines[i].handle(&change_bytes).unwrap();
    }
    assert!(
        is_owner(&engines[3], &g, &owner),
        "участник 3 не видел выборы"
    );

    let replay = engines[1]
        .owner_change_envelope(&g)
        .unwrap()
        .expect("есть OwnerChange");
    engines[3].handle(&enc(&replay)).unwrap();
    assert!(
        is_owner(&engines[3], &g, &engines[1].peer_id()),
        "офлайн-участник догнал владельца"
    );
    let _ = keys;
}

#[test]
fn two_person_challenge_owner_can_keep_immediately() {
    let (mut engines, keys, g) = build_group(2);
    let owner = engines[0].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 3600;
    set_now(&mut engines, t);

    let claim = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .expect("вызов в группе из 2");
    pump(&mut engines, &keys, vec![(1, claim)]);

    // Владелец не ждёт 24 часа: в паре согласие требуется сразу.
    let keep = engines[0]
        .owner_vote(&g, &[], true, true)
        .unwrap()
        .expect("владелец голосует сразу");
    pump(&mut engines, &keys, vec![(0, keep)]);
    for e in engines.iter_mut() {
        assert!(
            e.finalize_owner_election(&g).unwrap().is_none(),
            "ничья: смены нет"
        );
    }

    for e in &engines {
        assert!(
            e.owner_election(&g).is_none(),
            "выборы завершились досрочно"
        );
        assert!(is_owner(e, &g, &owner), "владелец остался");
    }
}

#[test]
fn two_person_challenge_owner_can_approve_immediately() {
    let (mut engines, keys, g) = build_group(2);
    let owner = engines[0].peer_id();
    let challenger = engines[1].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 3600;
    set_now(&mut engines, t);

    let claim = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .expect("вызов в группе из 2");
    let cid = claim_id(&claim);
    pump(&mut engines, &keys, vec![(1, claim)]);

    let accept = engines[0]
        .owner_vote(&g, &cid, true, false)
        .unwrap()
        .expect("владелец сразу соглашается");
    pump(&mut engines, &keys, vec![(0, accept)]);

    let change = engines[0]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("досрочная передача");
    pump(&mut engines, &keys, vec![(0, change)]);

    for e in &engines {
        assert!(is_owner(e, &g, &challenger), "согласие владельца сработало");
        assert!(e.owner_election(&g).is_none());
    }
    let _ = owner;
}

#[test]
fn two_person_challenge_can_repeat_immediately_after_finish() {
    let (mut engines, keys, g) = build_group(2);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 3600;
    set_now(&mut engines, t);

    let claim = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .expect("первый вызов");
    pump(&mut engines, &keys, vec![(1, claim)]);

    let keep = engines[0]
        .owner_vote(&g, &[], true, true)
        .unwrap()
        .expect("владелец оставляет владение");
    pump(&mut engines, &keys, vec![(0, keep)]);
    for e in engines.iter_mut() {
        assert!(e.finalize_owner_election(&g).unwrap().is_none());
    }

    // 7-дневный cooldown для пары отключён; важно только отсутствие активных выборов.
    let again = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .expect("повторный вызов сразу после завершения");
    let cid = claim_id(&again);
    assert!(!cid.is_empty());
}

#[test]
fn two_person_challenge_starts_and_owner_keeps() {
    let (mut engines, keys, g) = build_group(2);
    let owner = engines[0].peer_id();
    let last = engines[1].owner_last_seen(&g);
    let t = last + 3600;
    set_now(&mut engines, t);

    let claim = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .expect("в группе из 2 вызов должен создаваться");
    pump(&mut engines, &keys, vec![(1, claim)]);

    set_now(&mut engines, t + DAY + 3600);
    let keep = engines[0]
        .owner_vote(&g, &[], true, true)
        .unwrap()
        .expect("владелец голосует за себя");
    pump(&mut engines, &keys, vec![(0, keep)]);

    set_now(&mut engines, t + 2 * DAY + 60);
    assert!(
        engines[1].finalize_owner_election(&g).unwrap().is_none(),
        "без согласия владельца смены нет"
    );
    assert!(is_owner(&engines[0], &g, &owner));
}

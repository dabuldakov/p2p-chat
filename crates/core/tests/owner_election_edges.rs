//! Граничные случаи выборов владельца: валидация подписей, окна, кворум.

mod common;

use common::*;
use p2pchat_core::gossip::Outgoing;
use p2pchat_core::proto::gossip::Body;
use p2pchat_core::proto::{Gossip, OwnerClaim, OwnerTrigger, OwnerVote, PeerInfo};
use p2pchat_core::GossipEngine;
use p2pchat_crypto::{content_id, Identity};
use prost::Message as _;

const DAY: u64 = 24 * 3600;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn set_now(engines: &mut [GossipEngine], t: u64) {
    for e in engines {
        e.set_test_now(Some(t));
    }
}

fn mark_active(engines: &mut [GossipEngine], keys: &[Vec<u8>], t: u64) {
    for e in engines.iter_mut() {
        for k in keys {
            e.peers_mut().upsert(PeerInfo {
                peer_id: content_id(k).to_vec(),
                endpoints: Vec::new(),
                last_seen: t,
                is_relay: false,
                hops: 1,
            });
        }
    }
}

fn claim_sign_bytes(c: &OwnerClaim) -> Vec<u8> {
    let mut m = b"p2pchat-owner-claim-v1".to_vec();
    m.extend(&c.group_id);
    m.extend(&c.claimant);
    m.extend(&c.claimant_key);
    m.extend(&(c.trigger as u32).to_be_bytes());
    m.extend(&c.last_seen_owner.to_be_bytes());
    m.extend(&c.created_at.to_be_bytes());
    m.extend(&c.nonce);
    m
}

fn vote_sign_bytes(v: &OwnerVote) -> Vec<u8> {
    let mut m = b"p2pchat-owner-vote-v1".to_vec();
    m.extend(&v.group_id);
    m.extend(&v.claim_id);
    m.extend(&v.voter);
    m.extend(&v.voter_key);
    m.push(v.approve as u8);
    m.push(v.keep as u8);
    m.extend(&v.at.to_be_bytes());
    m
}

#[allow(clippy::too_many_arguments)]
fn make_claim_raw(
    claimant: Vec<u8>,
    key: Vec<u8>,
    group: &[u8],
    trigger: i32,
    last_seen: u64,
    created: u64,
    nonce: u8,
    sign: impl Fn(&[u8]) -> Vec<u8>,
) -> OwnerClaim {
    let mut c = OwnerClaim {
        group_id: group.to_vec(),
        claim_id: Vec::new(),
        claimant,
        claimant_key: key,
        trigger,
        last_seen_owner: last_seen,
        created_at: created,
        nonce: vec![nonce; 16],
        sig: Vec::new(),
    };
    let b = claim_sign_bytes(&c);
    c.claim_id = content_id(&b).to_vec();
    c.sig = sign(&b);
    c
}

fn make_claim(e: &GossipEngine, group: &[u8], trigger: i32, last: u64, created: u64) -> OwnerClaim {
    make_claim_raw(
        e.peer_id(),
        e.public_key(),
        group,
        trigger,
        last,
        created,
        7,
        |b| e.sign(b),
    )
}

#[allow(clippy::too_many_arguments)]
fn make_vote_raw(
    group: &[u8],
    claim_id: Vec<u8>,
    voter: Vec<u8>,
    key: Vec<u8>,
    approve: bool,
    keep: bool,
    at: u64,
    sign: impl Fn(&[u8]) -> Vec<u8>,
) -> OwnerVote {
    let mut v = OwnerVote {
        group_id: group.to_vec(),
        claim_id,
        voter,
        voter_key: key,
        approve,
        keep,
        at,
        sig: Vec::new(),
    };
    v.sig = sign(&vote_sign_bytes(&v));
    v
}

fn make_vote(
    e: &GossipEngine,
    group: &[u8],
    claim_id: &[u8],
    approve: bool,
    keep: bool,
    at: u64,
) -> OwnerVote {
    make_vote_raw(
        group,
        if keep { Vec::new() } else { claim_id.to_vec() },
        e.peer_id(),
        e.public_key(),
        approve,
        keep,
        at,
        |b| e.sign(b),
    )
}

fn cid(out: &Outgoing) -> Vec<u8> {
    match Gossip::decode(out.envelope.payload.as_slice())
        .unwrap()
        .body
    {
        Some(Body::OwnerClaim(c)) => c.claim_id,
        _ => panic!("not claim"),
    }
}

fn set_meta(e: &mut GossipEngine, key: &str, value: &[u8]) {
    e.log_mut().set_meta(key, value).unwrap();
}

#[test]
fn claim_validation_edges() {
    let (mut engines, keys, g) = build_group(3);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);
    let mut bad = make_claim(&engines[1], &g, OwnerTrigger::Absent as i32, last, t);
    bad.sig.truncate(8);
    engines[0].on_owner_claim(bad, &[]).unwrap();

    let mut bad = make_claim(&engines[1], &g, OwnerTrigger::Absent as i32, last, t);
    bad.claimant_key[0] ^= 1;
    engines[0].on_owner_claim(bad, &[]).unwrap();

    let mut bad = make_claim(&engines[1], &g, OwnerTrigger::Absent as i32, last, t);
    bad.claim_id[0] ^= 1;
    engines[0].on_owner_claim(bad, &[]).unwrap();

    let mut bad = make_claim(&engines[1], &g, OwnerTrigger::Absent as i32, last, t);
    bad.sig[0] ^= 1;
    engines[0].on_owner_claim(bad, &[]).unwrap();

    // Владелец не может заявить права.
    let own = make_claim(&engines[0], &g, OwnerTrigger::Challenge as i32, 0, t);
    engines[0].on_owner_claim(own, &[]).unwrap();

    // Не участник.
    let outsider = Identity::generate();
    let out_claim = make_claim_raw(
        outsider.peer_id().to_vec(),
        outsider.public_key().to_bytes().to_vec(),
        &g,
        OwnerTrigger::Absent as i32,
        last,
        t,
        2,
        |b| outsider.sign(b),
    );
    engines[0].on_owner_claim(out_claim, &[]).unwrap();

    // Неизвестная группа.
    let unknown = make_claim(&engines[1], b"nope", OwnerTrigger::Absent as i32, last, t);
    engines[0].on_owner_claim(unknown, &[]).unwrap();

    // Устаревшая заявка.
    let stale = make_claim(&engines[1], &g, OwnerTrigger::Absent as i32, last, t - 3600);
    engines[0].on_owner_claim(stale, &[]).unwrap();

    // Локально владелец активен, заявка «отсутствие» отклоняется.
    set_now(&mut engines, last + 3600);
    let early = make_claim(&engines[1], &g, OwnerTrigger::Absent as i32, 0, last + 3600);
    engines[0].on_owner_claim(early, &[]).unwrap();

    // AUTO слишком рано (7 дней + 1 час, нужно 8 дней).
    let t_auto = last + 7 * DAY + 3600;
    set_now(&mut engines, t_auto);
    let auto = make_claim(&engines[1], &g, OwnerTrigger::Auto as i32, last, t_auto);
    engines[0].on_owner_claim(auto, &[]).unwrap();

    // Неизвестный триггер.
    let mut trig = make_claim(&engines[1], &g, 99, last, t);
    trig.claim_id = content_id(&claim_sign_bytes(&trig)).to_vec();
    trig.sig = engines[1].sign(&claim_sign_bytes(&trig));
    engines[0].on_owner_claim(trig, &[]).unwrap();

    // Cooldown после провала.
    set_meta(
        &mut engines[0],
        &format!("owner_fail:{}", hex(&g)),
        &t.to_le_bytes(),
    );
    let fail = make_claim(&engines[1], &g, OwnerTrigger::Absent as i32, last, t);
    engines[0].on_owner_claim(fail, &[]).unwrap();

    // Cooldown вызова.
    set_meta(
        &mut engines[0],
        &format!("owner_challenge:{}", hex(&g)),
        &t.to_le_bytes(),
    );
    let chal = make_claim(&engines[1], &g, OwnerTrigger::Challenge as i32, 0, t);
    engines[0].on_owner_claim(chal, &[]).unwrap();
    let _ = keys;
}

#[test]
fn create_claim_rejections() {
    let mut empty = engine();
    assert!(empty
        .create_owner_claim(b"nope", OwnerTrigger::Challenge)
        .unwrap()
        .is_none());

    let (mut engines, keys, g) = build_group(3);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);
    assert!(engines[0]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .is_none());
    set_now(&mut engines, last + 3600);
    assert!(engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .is_none());
    set_now(&mut engines, t);
    let _ = keys;

    // cooldown провала
    let (mut engines, keys, g) = build_group(3);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);
    set_meta(
        &mut engines[1],
        &format!("owner_fail:{}", hex(&g)),
        &t.to_le_bytes(),
    );
    assert!(engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .is_none());
    assert!(engines[1]
        .create_owner_claim(&g, OwnerTrigger::Auto)
        .unwrap()
        .is_none());
    assert!(engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .is_some());
    let _ = keys;

    // AUTO слишком рано, не наследник, cooldown вызова.
    let (mut engines, keys, g) = build_group(3);
    let last = engines[1].owner_last_seen(&g);
    let early_t = last + 7 * DAY + 3600;
    set_now(&mut engines, early_t);
    mark_active(&mut engines, &keys, early_t);
    assert!(engines[1]
        .create_owner_claim(&g, OwnerTrigger::Auto)
        .unwrap()
        .is_none());
    let t = last + 8 * DAY + 60;
    set_now(&mut engines, t);
    mark_active(&mut engines, &keys, t);
    let mut nonowners: Vec<usize> = (1..3).collect();
    nonowners.sort_by(|a, b| engines[*a].peer_id().cmp(&engines[*b].peer_id()));
    assert!(engines[nonowners[1]]
        .create_owner_claim(&g, OwnerTrigger::Auto)
        .unwrap()
        .is_none());
    set_meta(
        &mut engines[nonowners[0]],
        &format!("owner_challenge:{}", hex(&g)),
        &t.to_le_bytes(),
    );
    assert!(engines[nonowners[0]]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .is_none());

    // Активная заявка: повторная заявка того же участника.
    let (mut engines, _keys, g) = build_group(3);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 3600;
    set_now(&mut engines, t);
    assert!(engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .is_some());
    assert!(engines[1]
        .create_owner_claim(&g, OwnerTrigger::Challenge)
        .unwrap()
        .is_none());
}

#[test]
fn vote_window_and_validation_edges() {
    let (mut engines, keys, g) = build_group(4);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);
    let out = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("claim");
    let claim = cid(&out);
    pump(&mut engines, &keys, vec![(1, out)]);

    // Нет выборов.
    let mut fresh = engine();
    assert!(fresh
        .owner_vote(b"nope", &claim, true, false)
        .unwrap()
        .is_none());
    assert!(fresh.finalize_owner_election(b"nope").unwrap().is_none());

    // Окно ещё не началось.
    assert!(engines[0]
        .owner_vote_at(&g, &claim, true, false, t)
        .unwrap()
        .is_none());
    assert!(engines[1]
        .finalize_owner_election_at(&g, t)
        .unwrap()
        .is_none());

    // on_owner_vote до открытия окна.
    set_now(&mut engines, t);
    let early_vote = make_vote(&engines[2], &g, &claim, true, false, t);
    engines[0].on_owner_vote(early_vote, &[]).unwrap();
    let vt = t + DAY + 60;
    set_now(&mut engines, vt);
    // Владелец не голосует в absent.
    assert!(engines[0]
        .owner_vote(&g, &claim, true, false)
        .unwrap()
        .is_none());
    // keep только в challenge.
    assert!(engines[2]
        .owner_vote(&g, &[], true, true)
        .unwrap()
        .is_none());
    // Нет такой заявки.
    assert!(engines[2]
        .owner_vote(&g, b"nope", true, false)
        .unwrap()
        .is_none());
    // Заявитель не голосует отдельно.
    assert!(engines[1]
        .owner_vote(&g, &claim, true, false)
        .unwrap()
        .is_none());

    let vote = engines[2]
        .owner_vote(&g, &claim, true, false)
        .unwrap()
        .unwrap();
    let voter = engines[2].peer_id();
    assert!(engines[2]
        .owner_vote(&g, &claim, true, false)
        .unwrap()
        .is_none());

    // Повторный голос уже после доставки.
    pump(&mut engines, &keys, vec![(2, vote)]);
    assert!(engines[2]
        .owner_vote(&g, &claim, true, false)
        .unwrap()
        .is_none());

    // on_owner_vote: нет выборов / до окна / плохая подпись / не участник.
    let bad_vote = make_vote(&engines[2], b"nope", &claim, true, false, vt);
    engines[0].on_owner_vote(bad_vote, &[]).unwrap();
    let early = make_vote(&engines[2], &g, &claim, true, false, t);
    engines[0].on_owner_vote(early, &[]).unwrap();

    let mut bad = make_vote(&engines[2], &g, &claim, true, false, vt);
    bad.sig[0] ^= 1;
    engines[0].on_owner_vote(bad, &[]).unwrap();

    let mut bad = make_vote(&engines[2], &g, &claim, true, false, vt);
    bad.voter_key[0] ^= 1;
    engines[0].on_owner_vote(bad, &[]).unwrap();

    let mut bad = make_vote(&engines[2], &g, &claim, true, false, vt);
    bad.claim_id.truncate(4);
    engines[0].on_owner_vote(bad, &[]).unwrap();

    let outsider = Identity::generate();
    let out_vote = make_vote_raw(
        &g,
        claim.clone(),
        outsider.peer_id().to_vec(),
        outsider.public_key().to_bytes().to_vec(),
        true,
        false,
        vt,
        |b| outsider.sign(b),
    );
    engines[0].on_owner_vote(out_vote, &[]).unwrap();

    // Некорректный timestamp.
    set_now(&mut engines, vt + 3600);
    let stale = make_vote(&engines[2], &g, &claim, true, false, vt);
    engines[0].on_owner_vote(stale, &[]).unwrap();
    let _ = voter;
}

#[test]
fn envelope_edges() {
    let mut e = engine();
    assert!(e.owner_change_envelope(b"g").unwrap().is_none());
    set_meta(&mut e, &format!("owner_change:{}", hex(b"g")), b"");
    assert!(e.owner_change_envelope(b"g").unwrap().is_none());
    set_meta(&mut e, &format!("owner_change:{}", hex(b"g")), b"x");
    assert!(e.owner_change_envelope(b"g").unwrap().is_none());

    let ch = p2pchat_core::proto::OwnerChange {
        group_id: b"other".to_vec(),
        ..Default::default()
    };
    set_meta(
        &mut e,
        &format!("owner_change:{}", hex(b"g")),
        &ch.encode_to_vec(),
    );
    assert!(e.owner_change_envelope(b"g").unwrap().is_none());
}

#[test]
fn tie_and_no_quorum() {
    let (mut engines, keys, g) = build_group(5);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);
    let a = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("a");
    pump(&mut engines, &keys, vec![(1, a)]);
    let b = engines[2]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .expect("b");
    pump(&mut engines, &keys, vec![(2, b)]);

    set_now(&mut engines, t + 2 * DAY + 60);
    assert!(
        engines[3].finalize_owner_election(&g).unwrap().is_none(),
        "два голоса при кворуме из четырёх — нет кворума"
    );

    // Ничья 1:1 при кворуме 2 (третий не голосует) — решает меньший peer_id.
    let (mut engines, keys, g) = build_group(4);
    let last = engines[1].owner_last_seen(&g);
    let t = last + 7 * DAY + 60;
    set_now(&mut engines, t);
    let a = engines[1]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .unwrap();
    pump(&mut engines, &keys, vec![(1, a)]);
    let b = engines[2]
        .create_owner_claim(&g, OwnerTrigger::Absent)
        .unwrap()
        .unwrap();
    pump(&mut engines, &keys, vec![(2, b)]);
    set_now(&mut engines, t + 2 * DAY + 60);
    let change = engines[3]
        .finalize_owner_election(&g)
        .unwrap()
        .expect("ничья разрешается порядком наследников");
    pump(&mut engines, &keys, vec![(3, change)]);
    let expected = engines[1].peer_id().min(engines[2].peer_id());
    for e in &engines {
        assert!(e.owner_of(&g).as_deref() == Some(expected.as_slice()));
    }
}

#[test]
fn on_change_basic_rejections() {
    let (mut engines, _keys, g) = build_group(3);
    let member = engines[0].public_key();
    let outsider = Identity::generate();
    let empty = p2pchat_core::proto::OwnerChange {
        group_id: g.clone(),
        ..Default::default()
    };
    let _ = engines[0].on_owner_change(empty.clone(), &outsider.public_key().to_bytes());
    engines[0].on_owner_change(empty, &member).unwrap();

    let last = engines[1].owner_last_seen(&g);
    let claim = make_claim(
        &engines[1],
        &g,
        OwnerTrigger::Absent as i32,
        last,
        last + 7 * DAY + 60,
    );
    let wrong_group = p2pchat_core::proto::OwnerChange {
        group_id: g.clone(),
        claim: Some(OwnerClaim {
            group_id: b"other".to_vec(),
            ..claim.clone()
        }),
        new_owner: claim.claimant.clone(),
        ..Default::default()
    };
    engines[0].on_owner_change(wrong_group, &member).unwrap();

    let fallback = p2pchat_core::proto::OwnerChange {
        group_id: g.clone(),
        claim: Some(claim.clone()),
        new_owner: claim.claimant.clone(),
        epoch: 0,
        ..Default::default()
    };
    engines[0].on_owner_change(fallback, &member).unwrap();
}

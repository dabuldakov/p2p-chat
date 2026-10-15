//! Rekey: после выхода участника оставшиеся меняют ключ, вышедший не читает новое.

#![allow(clippy::needless_range_loop)]

use std::collections::VecDeque;

use prost::Message as _;

use p2pchat_core::gossip::Outgoing;
use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;

fn enc(out: &Outgoing) -> Vec<u8> {
    out.envelope.encode_to_vec()
}

fn pump(engines: &mut [GossipEngine], keys: &[Vec<u8>], seed: Vec<(usize, Outgoing)>) {
    let n = engines.len();
    let mut queue: VecDeque<(usize, Outgoing)> = seed.into();
    let mut guard = 0;
    let mut seen: std::collections::HashSet<(usize, Vec<u8>)> = Default::default();
    while let Some((from, out)) = queue.pop_front() {
        guard += 1;
        assert!(guard < 20000, "gossip did not settle");
        let bytes = out.envelope.encode_to_vec();
        let targets: Vec<usize> = if out.targets.is_empty() {
            (0..n).filter(|&i| i != from).collect()
        } else {
            out.targets
                .iter()
                .filter_map(|pk| keys.iter().position(|k| k == pk))
                .collect()
        };
        for i in targets {
            if i == from {
                continue;
            }
            if !seen.insert((i, bytes.clone())) {
                continue;
            }
            let outcome = engines[i].handle(&bytes).expect("handle");
            for o in outcome.outgoing {
                queue.push_back((i, o));
            }
        }
    }
}

fn build_group(n: usize) -> (Vec<GossipEngine>, Vec<Vec<u8>>, Vec<u8>) {
    let mut engines: Vec<GossipEngine> =
        (0..n).map(|_| GossipEngine::new(Identity::generate(), Log::new())).collect();
    let g = b"team".to_vec();
    let _key = engines[0].create_group(g.clone());
    let invite = engines[0].invite_for(&g).unwrap();
    for e in engines.iter_mut().skip(1) {
        e.join_from_invite_full(&invite).unwrap();
    }
    let joins: Vec<Vec<u8>> = engines
        .iter()
        .map(|e| e.join_envelope(&g).unwrap().unwrap().envelope.encode_to_vec())
        .collect();
    for i in 0..n {
        for j in 0..n {
            if i != j {
                engines[i].handle(&joins[j]).unwrap();
            }
        }
    }
    let keys: Vec<Vec<u8>> = engines.iter().map(|e| e.public_key()).collect();
    (engines, keys, g)
}

#[test]
fn leaver_cannot_read_after_rekey() {
    let (mut engines, keys, g) = build_group(3);
    let leaver = 2;

    // вышедший рассылает событие выхода; в сети срабатывает rekey
    let leave = engines[leaver].leave_group(&g).unwrap().unwrap();
    pump(&mut engines, &keys, vec![(leaver, leave)]);

    // отправитель (индекс 0) пишет сообщение
    let out = engines[0].post(g.clone(), "после rekey".into()).unwrap();
    let bytes = enc(&out);

    // доставляем оставшемуся (индекс 1) и вышедшему (2) напрямую
    let _ = engines[1].handle(&bytes).unwrap();
    let _ = engines[2].handle(&bytes).unwrap();

    let b_has = engines[1].log().ordered(&g).iter().any(|m| m.text == "после rekey");
    let c_has = engines[2].log().ordered(&g).iter().any(|m| m.text == "после rekey");
    assert!(b_has, "оставшийся участник читает новое сообщение");
    assert!(!c_has, "вышедший не может прочитать новое сообщение");
}

#[test]
fn members_share_key_before_leave() {
    let (mut engines, keys, g) = build_group(3);
    let out = engines[0].post(g.clone(), "общее".into()).unwrap();
    let bytes = enc(&out);
    for i in 1..3 {
        let _ = engines[i].handle(&bytes).unwrap();
    }
    for i in 1..3 {
        assert!(engines[i].log().ordered(&g).iter().any(|m| m.text == "общее"));
    }
    let _ = keys;
}
#[test]
fn removed_member_cannot_read() {
    let (mut engines, keys, g) = build_group(3);
    assert!(engines[0].is_owner(&g).unwrap(), "создатель — владелец");

    let victim = engines[2].peer_id();
    let outs = engines[0].remove_member(&g, &victim).unwrap();
    let seed: Vec<(usize, Outgoing)> = outs.into_iter().map(|o| (0usize, o)).collect();
    pump(&mut engines, &keys, seed);

    let out = engines[0].post(g.clone(), "после удаления".into()).unwrap();
    let bytes = enc(&out);
    let _ = engines[1].handle(&bytes).unwrap();
    let _ = engines[2].handle(&bytes).unwrap();

    assert!(engines[1].log().ordered(&g).iter().any(|m| m.text == "после удаления"));
    assert!(!engines[2].log().ordered(&g).iter().any(|m| m.text == "после удаления"));
}

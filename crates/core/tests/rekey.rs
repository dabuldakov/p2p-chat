//! Удаление участника (MLS Remove/Commit): оставшиеся меняют эпоху,
//! удалённый новые сообщения не читает (forward secrecy).

mod common;

use common::*;

#[test]
fn members_read_before_removal() {
    let (mut engines, keys, g) = build_group(3);
    let bytes = enc(&engines[0].post(g.clone(), "общее".into()).unwrap());
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
    pump(&mut engines, &keys, outs.into_iter().map(|o| (0usize, o)).collect());

    let bytes = enc(&engines[0].post(g.clone(), "после удаления".into()).unwrap());
    let _ = engines[1].handle(&bytes).unwrap();
    let _ = engines[2].handle(&bytes).unwrap();

    assert!(engines[1].log().ordered(&g).iter().any(|m| m.text == "после удаления"));
    assert!(!engines[2].log().ordered(&g).iter().any(|m| m.text == "после удаления"));
}

//! Heartbeat владельца и last_seen.

mod common;

use common::*;

#[test]
fn owner_heartbeat_updates_last_seen() {
    let (mut engines, keys, g) = build_group(2);
    let before = engines[1].owner_last_seen(&g);
    assert!(before > 0, "при создании/входе last_seen выставлен");

    // не-владелец не может слать heartbeat
    assert!(engines[1].owner_alive(&g).unwrap().is_none());

    std::thread::sleep(std::time::Duration::from_secs(1));
    let out = engines[0].owner_alive(&g).unwrap().expect("heartbeat");
    pump(&mut engines, &keys, vec![(0, out)]);

    let after = engines[1].owner_last_seen(&g);
    assert!(after >= before);
}

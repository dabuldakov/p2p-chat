//! Приглашения: одноразовое и с лимитом входов.

mod common;

use common::{enc, engine};
use p2pchat_core::GossipEngine;

/// Попытка входа по билету. Возвращает true, если новичок вошёл.
fn try_join(host: &mut GossipEngine, invite: &str) -> bool {
    let mut joiner = engine();
    let Ok((g, req)) = joiner.join_request(invite) else {
        return false;
    };
    let Ok(out) = host.handle(&enc(&req)) else {
        return false;
    };
    for o in out.outgoing {
        let _ = joiner.handle(&enc(&o));
    }
    joiner.has_group(&g)
}

#[test]
fn single_use_allows_one() {
    let mut host = engine();
    let g = b"team".to_vec();
    host.create_group(g.clone()).unwrap();
    let invite = host.create_invite(&g, 3600, 1).unwrap();

    assert!(try_join(&mut host, &invite), "первый вход");
    assert!(!try_join(&mut host, &invite), "второй вход отклонён");
}

#[test]
fn multi_use_limit_five() {
    let mut host = engine();
    let g = b"team".to_vec();
    host.create_group(g.clone()).unwrap();
    let invite = host.create_invite(&g, 3600, 5).unwrap();

    for i in 1..=5 {
        assert!(try_join(&mut host, &invite), "вход {i}");
    }
    assert!(!try_join(&mut host, &invite), "шестой вход отклонён");
}

#[test]
fn expired_invite_rejected() {
    let mut host = engine();
    let g = b"team".to_vec();
    host.create_group(g.clone()).unwrap();
    // срок в прошлом
    let invite = host.create_invite(&g, 0, 1).unwrap(); // 0 = без срока
    // делаем просроченный вручную через билет с tiny ttl
    let invite2 = host.create_invite(&g, 1, 1).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));
    let _ = invite; // без срока — не проверяем
    assert!(!try_join(&mut host, &invite2), "просроченный отклонён");
}

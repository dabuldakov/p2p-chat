//! Проверка группового шифрования payload.

use prost::Message as _;

use p2pchat_core::{GossipEngine, Log};
use p2pchat_crypto::Identity;

fn enc(env: &p2pchat_core::proto::Envelope) -> Vec<u8> {
    env.encode_to_vec()
}

#[test]
fn members_decrypt_nonmembers_do_not() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());
    let mut c = GossipEngine::new(Identity::generate(), Log::new());
    let g = b"secret-group".to_vec();

    // A создаёт группу и раздаёт ключ B (C ключа не получает).
    let key = a.create_group(g.clone());
    b.join_group(g.clone(), key);

    let env = a.post(g.clone(), "top secret".into()).unwrap();
    let wire = enc(&env);

    // на проводе не должно быть открытого текста
    assert!(
        !wire.windows(10).any(|w| w == b"top secret"),
        "plaintext leaked on wire"
    );

    // B читает
    let ob = b.handle(&wire).unwrap();
    assert_eq!(ob.new_messages.len(), 1);
    assert_eq!(b.log().ordered(&g)[0].text, "top secret");

    // C не член группы — сообщение не появляется, но и ошибки нет
    let oc = c.handle(&wire).unwrap();
    assert!(oc.new_messages.is_empty());
    assert_eq!(c.log().len(), 0);
}

#[test]
fn wrong_key_fails_to_decrypt() {
    let mut a = GossipEngine::new(Identity::generate(), Log::new());
    let mut b = GossipEngine::new(Identity::generate(), Log::new());
    let g = b"g".to_vec();

    let _key = a.create_group(g.clone());
    // B вступает с другим (неверным) ключом
    b.join_group(g.clone(), [7u8; 32]);

    let env = a.post(g.clone(), "hi".into()).unwrap();
    let out = b.handle(&enc(&env)).unwrap();
    assert!(out.new_messages.is_empty(), "чужой ключ не должен расшифровать");
}
//! Подписанная жалоба: поля и проверка подписи.

use ed25519_dalek::VerifyingKey;
use prost::Message as _;
use p2pchat_ffi::ChatCore;

fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(format!("p2pchat-report-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p.join("chat.db").to_str().unwrap().to_string()
}

#[test]
fn report_is_signed_and_has_fields() {
    let a = ChatCore::new(tmp("a")).unwrap();
    let reported = vec![7u8; 32];
    let rep = a
        .create_report(
            reported.clone(),
            b"group".to_vec(),
            b"msg".to_vec(),
            "spam".into(),
            "hello".into(),
            vec![1, 2, 3],
        )
        .unwrap();

    let r = p2pchat_core::proto::Report::decode(rep.payload.as_slice()).unwrap();
    assert_eq!(r.reported_peer, reported);
    assert_eq!(r.group_id, b"group");
    assert_eq!(r.message_id, b"msg");
    assert_eq!(r.reason, "spam");
    assert_eq!(r.note, "hello");
    assert_eq!(r.screenshot, vec![1, 2, 3]);
    assert_eq!(r.nonce.len(), 16);
    assert_eq!(r.reporter_key, a.public_key());

    let vk = VerifyingKey::from_bytes(&r.reporter_key.clone().try_into().unwrap()).unwrap();
    p2pchat_crypto::verify(&vk, &rep.payload, &rep.signature).unwrap();

    let mut bad = rep.signature.clone();
    bad[0] ^= 1;
    assert!(p2pchat_crypto::verify(&vk, &rep.payload, &bad).is_err());
}

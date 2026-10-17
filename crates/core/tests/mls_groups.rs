//! Обёртка `MlsGroups` (карта групп + личность): создание, впуск, обмен.

use p2pchat_core::mls::{MlsGroups, Received};
use p2pchat_crypto::Identity;

#[test]
fn two_groups_wrappers_exchange() {
    let a = Identity::generate();
    let b = Identity::generate();

    let mut ma = MlsGroups::from_crypto(&a).expect("a");
    let mut mb = MlsGroups::from_crypto(&b).expect("b");

    ma.create(b"team").expect("create");
    assert!(ma.has(b"team"));

    let b_kp = mb.key_package().expect("kp");
    let (_commit, welcome) = ma.add_member(b"team", &b_kp).expect("add");
    let gid = mb.join(&welcome).expect("join");
    assert_eq!(gid, b"team".to_vec());
    assert_eq!(mb.members(b"team").len(), 2);

    let ct = ma.encrypt(b"team", b"hi bob").expect("enc");
    assert_eq!(
        mb.process(b"team", &ct).expect("proc"),
        Received::Application(b"hi bob".to_vec())
    );

    // ответ обратно
    let ct2 = mb.encrypt(b"team", b"hi alice").expect("enc2");
    assert_eq!(
        ma.process(b"team", &ct2).expect("proc2"),
        Received::Application(b"hi alice".to_vec())
    );
}

//! Проверка, что наша Ed25519-личность годится в MLS account/signer.

use p2pchat_core::mls::identity_from;
use p2pchat_crypto::Identity;
use p2pchat_mls::{add_member, create_group, decrypt, encrypt, join, key_package};

#[test]
fn our_identity_works_as_mls_signer() {
    let a = Identity::generate();
    let b = Identity::generate();

    let a_mls = identity_from(&a).expect("mls a");
    let b_mls = identity_from(&b).expect("mls b");

    let b_kp = key_package(&b_mls).expect("kp");
    let mut ga = create_group(&a_mls, b"g").expect("group");
    let (_c, welcome) = add_member(&mut ga, &a_mls, &b_kp).expect("add");
    let mut gb = join(&b_mls, &welcome).expect("join");

    let ct = encrypt(&mut ga, &a_mls, b"hi").expect("enc");
    assert_eq!(
        decrypt(&mut gb, &b_mls, &ct).expect("dec").as_deref(),
        Some(&b"hi"[..])
    );
}

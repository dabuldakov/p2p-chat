//! Групповое шифрование (MLS): участники читают, посторонние — нет.

mod common;

use common::*;

#[test]
fn members_decrypt_nonmembers_do_not() {
    let (mut engines, _keys, g) = build_group(2);
    let mut outsider = engine();

    let env = engines[0].post(g.clone(), "top secret".into()).unwrap();
    let wire = enc(&env);

    // на проводе нет открытого текста
    assert!(
        !wire.windows(10).any(|w| w == b"top secret"),
        "plaintext leaked on wire"
    );

    // участник читает
    engines[1].handle(&wire).unwrap();
    assert_eq!(engines[1].log().ordered(&g).len(), 1);
    assert_eq!(engines[1].log().ordered(&g)[0].text, "top secret");

    // посторонний — нет (и без ошибки)
    let oc = outsider.handle(&wire).unwrap();
    assert!(oc.new_messages.is_empty());
    assert_eq!(outsider.log().len(), 0);
}

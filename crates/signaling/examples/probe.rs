//! Диагностический WS-клиент: регистрируется под заданным ключом и,
//! получив Offer от телефона, отвечает Answer. Запуск:
//!   cargo run -p p2pchat-signaling --example probe -- <url> <my_key_hex> <phone_key_hex>

use futures_util::{SinkExt, StreamExt};
use prost::Message as _;

use p2pchat_proto::signal_envelope::Body;
use p2pchat_proto::{Answer, Register, SignalEnvelope};
use tokio_tungstenite::tungstenite::Message;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn hex4(b: &[u8]) -> String {
    b.iter().take(4).map(|x| format!("{:02x}", x)).collect()
}

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    let url = std::env::args().nth(1).unwrap();
    let my = hex(&std::env::args().nth(2).unwrap());
    let phone = hex(&std::env::args().nth(3).unwrap());

    let (ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (mut w, mut r) = ws.split();

    let reg = SignalEnvelope {
        from: my.clone(),
        to: vec![],
        body: Some(Body::Register(Register {
            peer_id: vec![],
            endpoints: vec![],
            is_relay: false,
        })),
    };
    w.send(Message::Binary(reg.encode_to_vec())).await.unwrap();
    println!("registered {} @ {url}", hex4(&my));

    while let Some(Ok(msg)) = r.next().await {
        if let Message::Binary(b) = msg {
            match SignalEnvelope::decode(b.as_ref()) {
                Ok(env) => {
                    let kind = match &env.body {
                        Some(Body::Register(_)) => "REGISTER",
                        Some(Body::Offer(_)) => "OFFER",
                        Some(Body::Answer(_)) => "ANSWER",
                        Some(Body::Ice(_)) => "ICE",
                        None => "NONE",
                    };
                    println!("recv {kind} from {}", hex4(&env.from));
                    if let Some(Body::Offer(_)) = &env.body {
                        let ans = SignalEnvelope {
                            from: my.clone(),
                            to: phone.clone(),
                            body: Some(Body::Answer(Answer {
                                sdp: b"PROBE-ANSWER".to_vec(),
                                session_id: "probe".into(),
                            })),
                        };
                        let _ = w.send(Message::Binary(ans.encode_to_vec())).await;
                        println!("-> sent ANSWER to phone {}", hex4(&phone));
                    }
                }
                Err(e) => println!("decode error: {e}"),
            }
        }
    }
}

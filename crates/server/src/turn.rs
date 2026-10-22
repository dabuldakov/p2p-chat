//! TURN REST API — выдача временных кред для coturn.
//!
//! coturn с `use-auth-secret` проверяет: `base64(HMAC-SHA1(secret, username))`
//! где `username` — это unix-время истечения (можно `expiry:userid`).
//! Секрет общий, но наружу не отдаётся: клиент получает только подпись с
//! коротким сроком жизни. Утёкшие креды быстро протухают.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha1::Sha1;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tracing::{info, warn};

type HmacSha1 = Hmac<Sha1>;

/// Сгенерировать временную пару логин/пароль.
pub fn turn_credentials(secret: &[u8], ttl: u64, user_id: &str) -> (String, String) {
    let expiry = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        + ttl;
    let username = if user_id.is_empty() {
        format!("{expiry}")
    } else {
        format!("{expiry}:{user_id}")
    };
    let mut mac = HmacSha1::new_from_slice(secret).expect("hmac key");
    mac.update(username.as_bytes());
    let credential = B64.encode(mac.finalize().into_bytes());
    (username, credential)
}

/// Мини-HTTP сервер: отдаёт JSON с временными кредами TURN.
/// Запускается на отдельном порту (по умолчанию 8091), за reverse-proxy.
pub async fn serve_creds(
    listener: TcpListener,
    secret: Arc<Vec<u8>>,
    public_host: String,
    ttl: u64,
) -> Result<()> {
    info!("turn-credentials endpoint on {}", listener.local_addr()?);
    loop {
        let (mut sock, _) = listener.accept().await?;
        let secret = secret.clone();
        let host = public_host.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req
                .lines()
                .next()
                .and_then(|l| l.split_whitespace().nth(1))
                .unwrap_or("/");

            let response = if path.starts_with("/turn-credentials") {
                let (username, credential) = turn_credentials(&secret, ttl, "");
                let body = format!(
                    r#"{{"username":"{username}","credential":"{credential}","ttl":{ttl},"uris":["turn:{host}:3478?transport=udp","turn:{host}:3478?transport=tcp"]}}"#
                );
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
            } else {
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
            };
            if sock.write_all(response.as_bytes()).await.is_err() {
                warn!("could not write turn-credentials response");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine;

    #[test]
    fn credential_matches_hmac() {
        let secret = b"topsecret";
        let (username, credential) = turn_credentials(secret, 3600, "");
        let mut mac = HmacSha1::new_from_slice(secret).unwrap();
        mac.update(username.as_bytes());
        let expect = B64.encode(mac.finalize().into_bytes());
        assert_eq!(credential, expect);
        // username — это unix-время в будущем
        let ts: u64 = username.parse().unwrap();
        assert!(ts > 1_000_000_000);
    }
    #[test]
    fn credential_with_user_id() {
        let (u, c) = turn_credentials(b"secret", 60, "bob");
        assert!(u.ends_with(":bob"));
        assert!(!c.is_empty());
    }

    #[tokio::test]
    async fn serve_creds_http() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve_creds(listener, Arc::new(b"secret".to_vec()), "turn.example".into(), 60));

        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"GET /turn-credentials HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = s.read(&mut buf).await.unwrap();
        let resp = String::from_utf8_lossy(&buf[..n]);
        assert!(resp.contains("200 OK"), "{resp}");
        assert!(resp.contains("turn:turn.example:3478"));
        assert!(resp.contains("\"ttl\":60"));

        let mut s2 = TcpStream::connect(addr).await.unwrap();
        s2.write_all(b"GET /nope HTTP/1.1\r\n\r\n").await.unwrap();
        let mut buf2 = vec![0u8; 1024];
        let n2 = s2.read(&mut buf2).await.unwrap();
        assert!(String::from_utf8_lossy(&buf2[..n2]).contains("404 Not Found"));
    }

}
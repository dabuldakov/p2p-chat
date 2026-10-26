//! Приём жалоб (UGC): `POST /report`.
//!
//! Тело: `{"report":"<base64 Report>","signature":"<base64 Ed25519>"}`.
//! Сервер проверяет подпись ключом отправителя, свежесть timestamp, лимиты и
//! складывает метаданные + скриншот в каталог. Сообщения чата сюда не попадают.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use prost::Message as _;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tracing::info;

use p2pchat_proto::Report;

const MAX_BODY: usize = 8 * 1024 * 1024;
const MAX_SCREENSHOT: usize = 2 * 1024 * 1024;
const MAX_NOTE: usize = 2000;
const MAX_TS_SKEW: u64 = 600;
const PER_KEY: usize = 5;
const PER_IP: usize = 30;
const WINDOW: Duration = Duration::from_secs(3600);

type KeyRate = Mutex<HashMap<Vec<u8>, Vec<Instant>>>;
type IpRate = Mutex<HashMap<String, Vec<Instant>>>;
type Dedup = Mutex<HashMap<(Vec<u8>, Vec<u8>, Vec<u8>), Instant>>;

#[derive(Clone)]
struct AppState {
    dir: PathBuf,
    per_key: Arc<KeyRate>,
    per_ip: Arc<IpRate>,
    seen: Arc<Dedup>,
}

#[derive(Deserialize)]
struct ReportBody {
    report: String,
    signature: String,
}

/// Проверка подписанной жалобы (чистая функция — удобно тестировать).
pub fn validate(payload: &[u8], signature: &[u8], now: u64) -> Result<Report, String> {
    let r = Report::decode(payload).map_err(|e| format!("decode: {e}"))?;
    if r.reporter_key.len() != 32 || r.reported_peer.len() != 32 || r.nonce.len() != 16 {
        return Err("bad key/peer/nonce".into());
    }
    if !matches!(r.reason.as_str(), "spam" | "abuse" | "illegal" | "other") {
        return Err("bad reason".into());
    }
    if r.note.chars().count() > MAX_NOTE || r.screenshot.len() > MAX_SCREENSHOT {
        return Err("too large".into());
    }
    if r.group_id.len() > 64 || r.message_id.len() > 64 {
        return Err("bad ids".into());
    }
    if now.abs_diff(r.timestamp) > MAX_TS_SKEW {
        return Err("stale timestamp".into());
    }
    p2pchat_crypto::verify_bytes(&r.reporter_key, payload, signature)
        .map_err(|_| "bad signature".to_string())?;
    Ok(r)
}

fn allow(map: &KeyRate, key: &[u8], max: usize, window: Duration) -> bool {
    let mut m = map.lock().unwrap();
    let v = m.entry(key.to_vec()).or_default();
    v.retain(|t| t.elapsed() < window);
    if v.len() >= max {
        return false;
    }
    v.push(Instant::now());
    true
}

fn allow_ip(map: &IpRate, key: &str) -> bool {
    let mut m = map.lock().unwrap();
    let v = m.entry(key.to_string()).or_default();
    v.retain(|t| t.elapsed() < WINDOW);
    if v.len() >= PER_IP {
        return false;
    }
    v.push(Instant::now());
    true
}

fn fresh_dedup(map: &Dedup, key: (Vec<u8>, Vec<u8>, Vec<u8>)) -> bool {
    let mut m = map.lock().unwrap();
    m.retain(|_, t| t.elapsed() < Duration::from_secs(24 * 3600));
    if m.contains_key(&key) {
        return false;
    }
    m.insert(key, Instant::now());
    true
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

async fn handle_report(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReportBody>,
) -> (StatusCode, Json<Value>) {
    let payload = match B64.decode(body.report.as_bytes()) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"error":"bad base64"}))),
    };
    let signature = match B64.decode(body.signature.as_bytes()) {
        Ok(v) => v,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(json!({"error":"bad base64"}))),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let report = match validate(&payload, &signature, now) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(json!({"error": e}))),
    };

    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .unwrap_or("unknown")
        .trim()
        .to_string();

    if !allow(&st.per_key, &report.reporter_key, PER_KEY, WINDOW) || !allow_ip(&st.per_ip, &ip) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error":"rate limit"})),
        );
    }

    let dedup = (
        report.reporter_key.clone(),
        report.reported_peer.clone(),
        report.message_id.clone(),
    );
    if !fresh_dedup(&st.seen, dedup) {
        return (StatusCode::OK, Json(json!({"ok": true, "duplicate": true})));
    }

    let name = format!("{}-{}", report.timestamp, hex(&report.nonce));
    let meta = json!({
        "timestamp": report.timestamp,
        "reason": report.reason,
        "note": report.note,
        "reporter_key": hex(&report.reporter_key),
        "reported_peer": hex(&report.reported_peer),
        "group_id": hex(&report.group_id),
        "message_id": hex(&report.message_id),
        "has_screenshot": !report.screenshot.is_empty(),
        "ip": ip,
    });
    let dir = st.dir.clone();
    let payload2 = payload.clone();
    let screenshot = report.screenshot.clone();
    let meta_bytes = serde_json::to_vec_pretty(&meta).unwrap_or_default();
    let _ = tokio::task::spawn_blocking(move || {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(format!("{name}.pb")), &payload2);
        let _ = std::fs::write(dir.join(format!("{name}.sig")), &signature);
        let _ = std::fs::write(dir.join(format!("{name}.json")), &meta_bytes);
        if !screenshot.is_empty() {
            let _ = std::fs::write(dir.join(format!("{name}.jpg")), &screenshot);
        }
    })
    .await;

    (StatusCode::OK, Json(json!({"ok": true})))
}

/// Запустить HTTP-эндпоинт жалоб.
pub async fn serve(addr: &str, dir: PathBuf) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    let st = AppState {
        dir: dir.clone(),
        per_key: Default::default(),
        per_ip: Default::default(),
        seen: Default::default(),
    };
    let app = Router::new()
        .route("/report", post(handle_report))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(st);
    info!(
        "report endpoint on {} (dir {})",
        listener.local_addr()?,
        dir.display()
    );
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use p2pchat_crypto::Identity;

    fn signed(reason: &str, ts: u64) -> (Vec<u8>, Vec<u8>) {
        let id = Identity::generate();
        let mut nonce = vec![0u8; 16];
        nonce[0] = 1;
        let r = Report {
            reporter_key: id.public_key().to_bytes().to_vec(),
            reported_peer: vec![7u8; 32],
            group_id: b"g".to_vec(),
            message_id: b"m".to_vec(),
            reason: reason.into(),
            note: "n".into(),
            screenshot: vec![1, 2, 3],
            timestamp: ts,
            nonce,
        };
        let payload = r.encode_to_vec();
        let sig = id.sign(&payload);
        (payload, sig)
    }

    #[test]
    fn validates_and_rejects() {
        let (p, s) = signed("spam", 1000);
        assert!(validate(&p, &s, 1000).is_ok());
        assert!(validate(&p, &s, 2000).is_err(), "старый timestamp");
        let mut bad = s.clone();
        bad[0] ^= 1;
        assert!(validate(&p, &bad, 1000).is_err(), "плохая подпись");

        let (p2, s2) = signed("bad", 1000);
        assert!(validate(&p2, &s2, 1000).is_err(), "неизвестная причина");
    }
}

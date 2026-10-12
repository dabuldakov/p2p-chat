//! Инфраструктурный signaling-сервер.
//!
//! Задачи:
//!   * WebSocket signaling — обмен offer/answer/ice (сообщений чата не видит).
//!   * TURN REST API — выдача временных кред для coturn (если задан TURN_SECRET).
//!
//! Запуск:
//!   p2pchat-signal-server --listen 0.0.0.0:8080
//! Переменные окружения (опционально):
//!   TURN_SECRET        — общий секрет с coturn (включает эндпоинт кред)
//!   TURN_PUBLIC_HOST   — публичный адрес TURN (напр. 90.188.89.63)
//!   TURN_TTL           — срок жизни кред в секундах (по умолчанию 3600)
//!   CREDS_LISTEN       — адрес HTTP-эндпоинта кред (по умолчанию 0.0.0.0:8091)

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use tokio::net::TcpListener;

#[derive(Parser, Debug)]
#[command(name = "p2pchat-signal-server", about = "WebRTC signaling server")]
struct Args {
    #[arg(long, default_value = "0.0.0.0:8080")]
    listen: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    // Опционально: эндпоинт выдачи временных TURN-кред.
    if let Ok(secret) = std::env::var("TURN_SECRET") {
        let host = std::env::var("TURN_PUBLIC_HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let ttl: u64 = std::env::var("TURN_TTL").ok().and_then(|v| v.parse().ok()).unwrap_or(3600);
        let listen = std::env::var("CREDS_LISTEN").unwrap_or_else(|_| "0.0.0.0:8091".into());
        let listener = TcpListener::bind(&listen).await?;
        let secret = Arc::new(secret.into_bytes());
        tokio::spawn(async move {
            if let Err(e) = p2pchat_signal_server::turn::serve_creds(listener, secret, host, ttl).await {
                tracing::error!("turn-credentials: {e}");
            }
        });
    } else {
        tracing::info!("TURN_SECRET not set — turn-credentials endpoint disabled");
    }

    p2pchat_signal_server::serve(&args.listen).await
}
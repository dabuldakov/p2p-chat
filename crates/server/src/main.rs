//! Инфраструктурный signaling-сервер.
//!
//! Единственная задача — помочь двум узлам обменяться `Offer`/`Answer`/`Ice`
//! для установки WebRTC-соединения. Сообщений чата не видит, ничего не хранит.
//!
//! Запуск:
//!   p2pchat-signal-server --listen 0.0.0.0:8080

use anyhow::Result;
use clap::Parser;

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
    p2pchat_signal_server::serve(&args.listen).await
}
//! Headless-узел p2pchat.
//!
//! Именно такой процесс поднимает «любой желающий» на VPS / Raspberry Pi /
//! старом телефоне. Он не владеет сетью — он один из равных. Отказ любого
//! узла не ломает оверлей: остальные продолжают gossip.
//!
//! Запуск:
//!   p2pchat-node --db node1.db --listen 0.0.0.0:9000
//!   p2pchat-node --db node2.db --listen 0.0.0.0:9001 --connect 127.0.0.1:9000

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use prost::Message as _;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, warn};

use p2pchat_core::{GossipEngine, Log};

#[derive(Parser, Debug)]
#[command(name = "p2pchat-node", about = "headless P2P chat node")]
struct Args {
    /// Файл SQLite для лога (или :memory:).
    #[arg(long, default_value = "node.db")]
    db: String,

    /// Адрес прослушивания.
    #[arg(long, default_value = "0.0.0.0:9000")]
    listen: String,

    /// К какому узлу подключиться на старте (можно несколько).
    #[arg(long)]
    connect: Vec<String>,

    /// Группа для stdin-сообщений.
    #[arg(long, default_value = "general")]
    group: String,
}

struct Node {
    engine: Mutex<GossipEngine>,
    outbound: broadcast::Sender<Vec<u8>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();

    let store = if args.db == ":memory:" {
        p2pchat_storage::Store::open_in_memory()?
    } else {
        p2pchat_storage::Store::open(&args.db)?
    };
    let identity = p2pchat_crypto::Identity::generate();
    let log = Log::with_store(store)?;
    let engine = GossipEngine::new(identity, log);

    info!("peer_id = {}", hex(&engine.peer_id()));
    info!("public_key = {}", hex(&engine.public_key()));

    let (outbound, _) = broadcast::channel::<Vec<u8>>(1024);
    let node = Arc::new(Node { engine: Mutex::new(engine), outbound });

    // слушаем входящие
    let listener = TcpListener::bind(&args.listen).await?;
    info!("listening on {}", args.listen);
    {
        let node = node.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, addr)) => {
                        info!("incoming from {addr}");
                        let node = node.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_conn(stream, node).await {
                                warn!("connection error: {e}");
                            }
                        });
                    }
                    Err(e) => error!("accept: {e}"),
                }
            }
        });
    }

    // исходящие подключения
    for addr in &args.connect {
        let node = node.clone();
        let addr = addr.clone();
        tokio::spawn(async move {
            match TcpStream::connect(&addr).await {
                Ok(stream) => {
                    info!("connected to {addr}");
                    if let Err(e) = handle_conn(stream, node).await {
                        warn!("connection error: {e}");
                    }
                }
                Err(e) => error!("connect {addr}: {e}"),
            }
        });
    }

    // stdin -> сообщения в группу
    let group = args.group.clone().into_bytes();
    let node_stdin = node.clone();
    tokio::task::spawn_blocking(move || {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let mut engine = node_stdin.engine.blocking_lock();
            match engine.post(group.clone(), line) {
                Ok(out) => {
                    let bytes = out.envelope.encode_to_vec();
                    let _ = node_stdin.outbound.send(bytes);
                }
                Err(e) => error!("post: {e}"),
            }
        }
    });

    tokio::signal::ctrl_c().await?;
    info!("shutting down");
    Ok(())
}

/// Обработка одного TCP-соединения.
/// Рукопожатие: обе стороны сразу шлют 32-байтовый публичный ключ.
/// Далее — кадры `[u32 len][Envelope bytes]`.
async fn handle_conn(stream: TcpStream, node: Arc<Node>) -> Result<()> {
    let (mut rd, mut wr) = stream.into_split();

    // наш ключ
    let (our_pk, our_id) = {
        let engine = node.engine.lock().await;
        (engine.public_key(), engine.peer_id())
    };
    wr.write_all(&our_pk).await?;
    wr.flush().await?;

    // их ключ
    let mut their_pk = [0u8; 32];
    rd.read_exact(&mut their_pk).await?;
    let their_id = peer_id_from_pk(&their_pk);
    info!("handshake: peer {} <-> {}", hex(&our_id), hex(&their_id));

    // писатель: транслируем всё, что попадает в outbound
    let mut rx = node.outbound.subscribe();
    let writer = tokio::spawn(async move {
        while let Ok(env) = rx.recv().await {
            if write_frame(&mut wr, &env).await.is_err() {
                break;
            }
        }
    });

    // читатель: кадры -> движок -> пересылка
    while let Ok(frame) = read_frame(&mut rd).await {
        let outgoing = {
            let mut engine = node.engine.lock().await;
            engine.handle(&frame)
        };
        match outgoing {
            Ok(outcome) => {
                for m in &outcome.new_messages {
                    info!("<- {}: {}", hex(&m.author[..4.min(m.author.len())]), m.text);
                }
                for env in outcome.outgoing {
                    let _ = node.outbound.send(env.envelope.encode_to_vec());
                }
            }
            Err(e) => warn!("bad envelope: {e}"),
        }
    }

    writer.abort();
    Ok(())
}

async fn write_frame<W: AsyncWriteExt + Unpin>(w: &mut W, data: &[u8]) -> std::io::Result<()> {
    w.write_all(&(data.len() as u32).to_be_bytes()).await?;
    w.write_all(data).await?;
    w.flush().await
}

async fn read_frame<R: AsyncReadExt + Unpin>(r: &mut R) -> std::io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    r.read_exact(&mut len).await?;
    let n = u32::from_be_bytes(len) as usize;
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

fn peer_id_from_pk(pk: &[u8; 32]) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(pk);
    h.finalize().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
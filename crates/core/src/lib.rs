//! p2pchat-core: протокол, CRDT-лог, gossip, таблица пиров.
//!
//! Крейт не знает ничего про транспорт и UI — только чистая логика.
//! Транспорт передаёт ему сырые байты `Envelope`, а он возвращает, что
//! и кому переслать.

pub mod error;
pub mod log;
pub mod membership;
pub mod merkle;
pub mod peers;
pub mod gossip;
pub mod clock;

/// Сгенерированный из protobuf код (общий для всех крейтов).
pub use p2pchat_proto as proto;

pub use error::CoreError;
pub use log::Log;
pub use membership::Membership;
pub use peers::PeerTable;
pub use gossip::GossipEngine;
pub use clock::Lamport;

use sha2::{Digest, Sha256};

/// Идентификаторы (32-байтовые хеши).
pub type MessageId = Vec<u8>;
pub type PeerId = Vec<u8>;
pub type GroupId = Vec<u8>;

/// Контент-адрес сообщения.
pub fn message_id(m: &proto::ChatMessage) -> MessageId {
    let mut h = Sha256::new();
    h.update(&m.group_id);
    h.update(&m.author);
    h.update(m.lamport.to_be_bytes());
    h.update(m.wall_time.to_be_bytes());
    h.update(m.text.as_bytes());
    for p in &m.parents {
        h.update(p);
    }
    h.finalize().to_vec()
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
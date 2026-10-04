use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("decode error: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("encode error: {0}")]
    Encode(#[from] prost::EncodeError),
    #[error("storage error: {0}")]
    Storage(#[from] p2pchat_storage::StorageError),
    #[error("crypto error: {0}")]
    Crypto(#[from] p2pchat_crypto::CryptoError),
    #[error("unknown peer: {0:?}")]
    UnknownPeer(Vec<u8>),
    #[error("bad envelope: {0}")]
    BadEnvelope(String),
}
//! Криптографическое ядро p2pchat.
//!
//! * [`Identity`] — Ed25519-личность узла; `peer_id` = SHA-256 публичного ключа.
//! * [`SenderKey`] — групповой ключ для шифрования сообщений (ChaCha20-Poly1305).
//! * Подписи сообщений и проверка целостности.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256, Sha512};
use thiserror::Error;
use x25519_dalek::{PublicKey as XPublicKey, StaticSecret};
use zeroize::Zeroize;

pub type PeerId = [u8; 32];

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("bad signature")]
    BadSignature,
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed")]
    Decrypt,
    #[error("bad key material")]
    BadKey,
}

/// Личность узла: долгоживущий Ed25519-ключ.
pub struct Identity {
    signing: SigningKey,
}

impl Identity {
    pub fn generate() -> Self {
        Self { signing: SigningKey::generate(&mut OsRng) }
    }

    pub fn from_secret_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        let arr: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::BadKey)?;
        Ok(Self { signing: SigningKey::from_bytes(&arr) })
    }

    pub fn secret_bytes(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    pub fn public_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    /// peer_id = SHA-256(public key)
    pub fn peer_id(&self) -> PeerId {
        peer_id_of(&self.public_key())
    }

    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        self.signing.sign(data).to_bytes().to_vec()
    }

    pub fn verify(&self, data: &[u8], sig: &[u8]) -> Result<(), CryptoError> {
        verify(&self.public_key(), data, sig)
    }
}

pub fn peer_id_of(pk: &VerifyingKey) -> PeerId {
    let mut h = Sha256::new();
    h.update(pk.as_bytes());
    h.finalize().into()
}

pub fn verify(pk: &VerifyingKey, data: &[u8], sig: &[u8]) -> Result<(), CryptoError> {
    let sig = Signature::from_slice(sig).map_err(|_| CryptoError::BadSignature)?;
    pk.verify(data, &sig).map_err(|_| CryptoError::BadSignature)
}

/// Групповой ключ отправителя (sender-key, как в WhatsApp/Matrix).
/// Каждый участник рассылает свой ключ остальным; все шифруют свои
/// сообщения производным ключом. Здесь — упрощённая реализация с
/// монотонным nonce/epoch.
pub struct SenderKey {
    key: [u8; 32],
    epoch: u32,
    counter: u64,
}

impl SenderKey {
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        rand::RngCore::fill_bytes(&mut OsRng, &mut key);
        Self { key, epoch: 0, counter: 0 }
    }

    pub fn from_bytes(key: [u8; 32], epoch: u32, counter: u64) -> Self {
        Self { key, epoch, counter }
    }

    pub fn export(&self) -> ([u8; 32], u32, u64) {
        (self.key, self.epoch, self.counter)
    }

    /// Шифрует сообщение, возвращая `(nonce, ciphertext)`.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<([u8; 12], Vec<u8>), CryptoError> {
        let nonce = self.next_nonce();
        let cipher = ChaCha20Poly1305::new((&self.key).into());
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: &[] })
            .map_err(|_| CryptoError::Encrypt)?;
        Ok((nonce, ct))
    }

    /// Расшифровывает сообщение. Nonce передаётся вместе с шифртекстом.
    pub fn decrypt(&self, nonce: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = ChaCha20Poly1305::new((&self.key).into());
        cipher
            .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: &[] })
            .map_err(|_| CryptoError::Decrypt)
    }

    fn next_nonce(&mut self) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&self.counter.to_be_bytes());
        self.counter += 1;
        nonce
    }
}

impl Drop for SenderKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// Общий групповой ключ: все участники владеют одним секретом и
/// шифруют/расшифровывают им сообщения группы. Nonce случайный —
/// коллизии пренебрежимо маловероятны на 96 битах.
pub struct GroupKey {
    key: [u8; 32],
}

impl GroupKey {
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        rand::RngCore::fill_bytes(&mut OsRng, &mut key);
        Self { key }
    }

    pub fn from_bytes(key: [u8; 32]) -> Self {
        Self { key }
    }

    pub fn export(&self) -> [u8; 32] {
        self.key
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<([u8; 12], Vec<u8>), CryptoError> {
        let mut nonce = [0u8; 12];
        rand::RngCore::fill_bytes(&mut OsRng, &mut nonce);
        let cipher = ChaCha20Poly1305::new((&self.key).into());
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: &[] })
            .map_err(|_| CryptoError::Encrypt)?;
        Ok((nonce, ct))
    }

    pub fn decrypt(&self, nonce: &[u8; 12], ciphertext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let cipher = ChaCha20Poly1305::new((&self.key).into());
        cipher
            .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: &[] })
            .map_err(|_| CryptoError::Decrypt)
    }
}

impl Drop for GroupKey {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// Вывод симметричного ключа из общего секрета (X25519 + HKDF).
pub fn derive_key(shared: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, shared);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm).expect("hkdf");
    okm
}

/// X25519-секрет, выведенный из Ed25519 seed (стандартное преобразование).
pub fn x25519_secret_bytes(seed: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha512::new();
    h.update(seed);
    let out = h.finalize();
    let mut sk = [0u8; 32];
    sk.copy_from_slice(&out[..32]);
    sk[0] &= 248;
    sk[31] &= 127;
    sk[31] |= 64;
    sk
}

impl Identity {
    pub fn x25519_secret(&self) -> [u8; 32] {
        x25519_secret_bytes(&self.signing.to_bytes())
    }
}

/// X25519-публичный ключ из Ed25519-публичного (birational map).
pub fn x25519_public(vk: &VerifyingKey) -> [u8; 32] {
    vk.to_montgomery().to_bytes()
}

/// Общий попарный ключ между нашей личностью и публичным ключом собеседника.
pub fn pairwise_key(id: &Identity, peer: &VerifyingKey) -> [u8; 32] {
    let sk = StaticSecret::from(id.x25519_secret());
    let pk = XPublicKey::from(x25519_public(peer));
    let shared = sk.diffie_hellman(&pk);
    derive_key(shared.as_bytes(), b"p2pchat-pairwise-v1")
}

/// Зашифровать данные личным ключом для конкретного собеседника.
pub fn pairwise_encrypt(
    id: &Identity,
    peer: &VerifyingKey,
    plaintext: &[u8],
) -> Result<([u8; 12], Vec<u8>), CryptoError> {
    let key = pairwise_key(id, peer);
    let mut nonce = [0u8; 12];
    rand::RngCore::fill_bytes(&mut OsRng, &mut nonce);
    let cipher = ChaCha20Poly1305::new((&key).into());
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), Payload { msg: plaintext, aad: &[] })
        .map_err(|_| CryptoError::Encrypt)?;
    Ok((nonce, ct))
}

pub fn pairwise_decrypt(
    id: &Identity,
    peer: &VerifyingKey,
    nonce: &[u8; 12],
    ciphertext: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let key = pairwise_key(id, peer);
    let cipher = ChaCha20Poly1305::new((&key).into());
    cipher
        .decrypt(Nonce::from_slice(nonce), Payload { msg: ciphertext, aad: &[] })
        .map_err(|_| CryptoError::Decrypt)
}

/// Контент-адресация: id = SHA-256(data).
pub fn content_id(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairwise_roundtrip_and_wrong_key() {
        let a = Identity::generate();
        let b = Identity::generate();
        let c = Identity::generate();
        let msg = b"new-group-key";

        let (nonce, ct) = pairwise_encrypt(&a, &b.public_key(), msg).unwrap();
        // B расшифровывает, зная публичный ключ A
        assert_eq!(pairwise_decrypt(&b, &a.public_key(), &nonce, &ct).unwrap(), msg);
        // C не может
        assert!(pairwise_decrypt(&c, &a.public_key(), &nonce, &ct).is_err());
    }
    #[test]
    fn identity_secret_roundtrip_and_signatures() {
        let id = Identity::generate();
        let id2 = Identity::from_secret_bytes(&id.secret_bytes()).unwrap();
        assert_eq!(id.peer_id(), id2.peer_id());
        assert_eq!(id.public_key(), id2.public_key());
        assert_eq!(peer_id_of(&id.public_key()), id.peer_id());

        let msg = b"payload";
        let sig = id.sign(msg);
        assert!(id.verify(msg, &sig).is_ok());
        assert!(verify(&id.public_key(), msg, &sig).is_ok());
        let mut bad = sig.clone();
        bad[0] ^= 1;
        assert!(id.verify(msg, &bad).is_err());
        assert!(verify(&id.public_key(), b"other", &sig).is_err());
        assert!(Identity::from_secret_bytes(&[0u8; 5]).is_err());
        assert!(verify(&id.public_key(), msg, b"short").is_err());
    }

    #[test]
    fn sender_key_roundtrip_and_wrong_key() {
        let mut k = SenderKey::generate();
        let (key, epoch, counter) = k.export();
        let k2 = SenderKey::from_bytes(key, epoch, counter);
        let (n, ct) = k.encrypt(b"hi").unwrap();
        assert_eq!(k2.decrypt(&n, &ct).unwrap(), b"hi");
        let other = SenderKey::from_bytes([7u8; 32], 0, 0);
        assert!(other.decrypt(&n, &ct).is_err());
    }

    #[test]
    fn group_key_roundtrip_and_wrong_key() {
        let k = GroupKey::generate();
        let k2 = GroupKey::from_bytes(k.export());
        let (n, ct) = k.encrypt(b"group").unwrap();
        assert_eq!(k2.decrypt(&n, &ct).unwrap(), b"group");
        let other = GroupKey::from_bytes([0u8; 32]);
        assert!(other.decrypt(&n, &ct).is_err());
    }

    #[test]
    fn kdf_x25519_and_ids_are_deterministic() {
        let seed = [3u8; 32];
        assert_eq!(x25519_secret_bytes(&seed), x25519_secret_bytes(&seed));
        assert_eq!(derive_key(b"shared", b"info"), derive_key(b"shared", b"info"));
        assert_ne!(derive_key(b"shared", b"info"), derive_key(b"shared", b"other"));

        let a = Identity::from_secret_bytes(&seed).unwrap();
        let b = Identity::generate();
        assert_eq!(pairwise_key(&a, &b.public_key()), pairwise_key(&b, &a.public_key()));
        assert_eq!(x25519_public(&a.public_key()), x25519_public(&a.public_key()));
        assert_eq!(a.x25519_secret(), a.x25519_secret());

        assert_eq!(content_id(b"x"), content_id(b"x"));
        assert_ne!(content_id(b"x"), content_id(b"y"));
    }
}

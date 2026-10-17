//! Инвайт-билет группы — одноразовая подписанная ссылка/QR.
//!
//! Формат: `p2pchat://join?g=<hex>&h=<hex>&n=<hex>&e=<u64>&s=<hex>`
//! * `g` — group_id;
//! * `h` — публичный ключ хозяина (для bootstrap-подключения);
//! * `n` — nonce (16 байт), обеспечивает одноразовость;
//! * `e` — срок действия (unix seconds; 0 = без срока);
//! * `s` — Ed25519-подпись хозяина над `p2pchat-ticket-v1 || g || h || n || e`.
//!
//! Ключа группы в билете НЕТ: ключ выдаётся попарно после принятия заявки.

use ed25519_dalek::VerifyingKey;

use crate::{CoreError, GroupId};
use p2pchat_crypto::{verify, Identity};

pub const SCHEME: &str = "p2pchat://join";
const DOMAIN: &[u8] = b"p2pchat-ticket-v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub group_id: GroupId,
    /// Публичный ключ хозяина (inviter).
    pub host: Vec<u8>,
    pub nonce: [u8; 16],
    /// Срок действия (unix seconds); 0 — без срока.
    pub expiry: u64,
    pub sig: Vec<u8>,
}

impl Invite {
    /// Создать одноразовый билет (подписывает хозяин).
    pub fn ticket(group_id: &[u8], host: &[u8], expiry: u64, signer: &Identity) -> Self {
        let mut nonce = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
        let sig = signer.sign(&signing_msg(group_id, host, &nonce, expiry));
        Self { group_id: group_id.to_vec(), host: host.to_vec(), nonce, expiry, sig }
    }

    pub fn encode(&self) -> String {
        format!(
            "{SCHEME}?g={}&h={}&n={}&e={}&s={}",
            hex(&self.group_id),
            hex(&self.host),
            hex(&self.nonce),
            self.expiry,
            hex(&self.sig)
        )
    }

    pub fn parse(s: &str) -> Result<Self, CoreError> {
        let query = s
            .split_once('?')
            .map(|(_, q)| q)
            .ok_or_else(|| CoreError::BadEnvelope("invite: no query".into()))?;

        let (mut g, mut h, mut n, mut e, mut sig) = (None, None, None, None, None);
        for part in query.split('&') {
            let mut it = part.splitn(2, '=');
            match (it.next(), it.next()) {
                (Some("g"), Some(v)) => g = Some(v.to_string()),
                (Some("h"), Some(v)) => h = Some(v.to_string()),
                (Some("n"), Some(v)) => n = Some(v.to_string()),
                (Some("e"), Some(v)) => e = Some(v.to_string()),
                (Some("s"), Some(v)) => sig = Some(v.to_string()),
                _ => {}
            }
        }

        let group_id = unhex(&g.ok_or_else(|| CoreError::BadEnvelope("invite: no group".into()))?)?;
        let host = unhex(&h.ok_or_else(|| CoreError::BadEnvelope("invite: no host".into()))?)?;
        let nonce_bytes = unhex(&n.ok_or_else(|| CoreError::BadEnvelope("invite: no nonce".into()))?)?;
        let nonce: [u8; 16] = nonce_bytes
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::BadEnvelope("invite: bad nonce".into()))?;
        let expiry: u64 = e
            .ok_or_else(|| CoreError::BadEnvelope("invite: no expiry".into()))?
            .parse()
            .map_err(|_| CoreError::BadEnvelope("invite: bad expiry".into()))?;
        let sig = unhex(&sig.ok_or_else(|| CoreError::BadEnvelope("invite: no sig".into()))?)?;

        Ok(Self { group_id, host, nonce, expiry, sig })
    }

    /// Проверить подпись хозяина.
    pub fn verify_sig(&self) -> bool {
        let Ok(pk_arr) = <[u8; 32]>::try_from(self.host.as_slice()) else {
            return false;
        };
        let Ok(vk) = VerifyingKey::from_bytes(&pk_arr) else {
            return false;
        };
        verify(&vk, &signing_msg(&self.group_id, &self.host, &self.nonce, self.expiry), &self.sig).is_ok()
    }

    /// Истёк ли билет (0 = без срока).
    pub fn is_expired(&self, now: u64) -> bool {
        self.expiry != 0 && now > self.expiry
    }

    /// Ключ для учёта использования билета (group + nonce).
    pub fn use_token(&self) -> String {
        format!("{}:{}", hex(&self.group_id), hex(&self.nonce))
    }
}

fn signing_msg(group_id: &[u8], host: &[u8], nonce: &[u8; 16], expiry: u64) -> Vec<u8> {
    let mut m = DOMAIN.to_vec();
    m.extend_from_slice(group_id);
    m.extend_from_slice(host);
    m.extend_from_slice(nonce);
    m.extend_from_slice(&expiry.to_be_bytes());
    m
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn unhex(s: &str) -> Result<Vec<u8>, CoreError> {
    if !s.len().is_multiple_of(2) {
        return Err(CoreError::BadEnvelope("invite: odd hex".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| CoreError::BadEnvelope("invite: bad hex".into()))
        })
        .collect()
}

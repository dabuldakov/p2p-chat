//! Инвайт группы — ссылка/QR с идентификатором группы, ключом и адресом
//! приглашающего. Чистый модуль без состояния: кодирование и разбор.
//!
//! Формат: `p2pchat://join?g=<hex>&k=<hex>&h=<hex>`
//! * `g` — group_id
//! * `k` — групповой ключ (32 байта)
//! * `h` — публичный ключ приглашающего (для bootstrap-подключения)

use crate::error::CoreError;
use crate::GroupId;

pub const SCHEME: &str = "p2pchat://join";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub group_id: GroupId,
    pub key: [u8; 32],
    /// Публичный ключ приглашающего. Пусто, если не задан.
    pub host: Vec<u8>,
}

impl Invite {
    pub fn new(group_id: &[u8], key: &[u8; 32], host: &[u8]) -> Self {
        Self { group_id: group_id.to_vec(), key: *key, host: host.to_vec() }
    }

    pub fn encode(&self) -> String {
        format!(
            "{SCHEME}?g={}&k={}&h={}",
            hex(&self.group_id),
            hex(&self.key),
            hex(&self.host)
        )
    }

    pub fn parse(s: &str) -> Result<Self, CoreError> {
        let query = s
            .split_once('?')
            .map(|(_, q)| q)
            .ok_or_else(|| CoreError::BadEnvelope("invite: no query".into()))?;

        let (mut g, mut k, mut h) = (None, None, None);
        for part in query.split('&') {
            let mut it = part.splitn(2, '=');
            match (it.next(), it.next()) {
                (Some("g"), Some(v)) => g = Some(v.to_string()),
                (Some("k"), Some(v)) => k = Some(v.to_string()),
                (Some("h"), Some(v)) => h = Some(v.to_string()),
                _ => {}
            }
        }

        let group_id = unhex(&g.ok_or_else(|| CoreError::BadEnvelope("invite: no group".into()))?)?;
        let key_bytes = unhex(&k.ok_or_else(|| CoreError::BadEnvelope("invite: no key".into()))?)?;
        let key: [u8; 32] = key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::BadEnvelope("invite: bad key size".into()))?;
        let host = match h {
            Some(v) => unhex(&v)?,
            None => Vec::new(),
        };
        Ok(Self { group_id, key, host })
    }
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
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return Err(CoreError::BadEnvelope("invite: bad hex length".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| CoreError::BadEnvelope("invite: bad hex".into())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_host() {
        let key = [7u8; 32];
        let inv = Invite::new(b"team", &key, &[1, 2, 3]);
        let s = inv.encode();
        assert!(s.starts_with(SCHEME));
        assert_eq!(Invite::parse(&s).unwrap(), inv);
    }

    #[test]
    fn parse_without_host_is_empty() {
        let s = "p2pchat://join?g=7465616d&k=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let inv = Invite::parse(s).unwrap();
        assert_eq!(inv.group_id, b"team");
        assert!(inv.host.is_empty());
    }

    #[test]
    fn bad_invites() {
        assert!(Invite::parse("nonsense").is_err());
        assert!(Invite::parse("p2pchat://join?g=zz").is_err());
        assert!(Invite::parse("p2pchat://join?g=00&k=00").is_err()); // ключ не 32 байта
    }
}
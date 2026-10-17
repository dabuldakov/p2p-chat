//! Мост между нашей личностью (`crypto::Identity`, Ed25519) и MLS.
//!
//! `credential.identity` = публичный ключ аккаунта; ключ подписи устройства =
//! та же Ed25519-личность (фаза A: одно устройство на аккаунт). На этапе B
//! сюда придёт отдельный device-ключ и сертификат устройства.

use p2pchat_crypto::Identity;
use p2pchat_mls::{from_parts, MlsError, MlsIdentity};

/// Построить MLS-личность из нашей.
pub fn identity_from(id: &Identity) -> Result<MlsIdentity, MlsError> {
    let account = id.public_key().to_bytes();
    let private = id.secret_bytes();
    from_parts(&account, &private, &account)
}

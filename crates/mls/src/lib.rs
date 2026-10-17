//! Обёртка над OpenMLS (RFC 9420) для группового E2EE.
//!
//! Даёт forward secrecy и post-compromise security: смена состава и обновления
//! ключей сдвигают эпоху, старые секреты удаляются. Байты сообщений/медиа ходят
//! поверх нашего gossip — тут только крипто-операции MLS.
//!
//! Модель: `credential.identity` = пользователь, `signer` = устройство. В фазе A
//! устройство одно, но модель сразу user/device-ориентированная.

use openmls::prelude::{tls_codec::*, *};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;

pub mod persist;
pub use persist::{load_group, load_identity, save};

/// Криптопровайдер (в нём хранится ключевой материал).
pub type Provider = OpenMlsRustCrypto;

/// Обязательная ciphersuite: X25519 + AES-GCM + Ed25519.
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

#[derive(Debug, thiserror::Error)]
pub enum MlsError {
    #[error("mls: {0}")]
    Mls(String),
    #[error("codec: {0}")]
    Codec(String),
    #[error("not a welcome message")]
    NotWelcome,
}

fn mls<T, E: std::fmt::Debug>(r: Result<T, E>) -> Result<T, MlsError> {
    r.map_err(|e| MlsError::Mls(format!("{e:?}")))
}

fn codec<T, E: std::fmt::Debug>(r: Result<T, E>) -> Result<T, MlsError> {
    r.map_err(|e| MlsError::Codec(format!("{e:?}")))
}

/// Личность MLS: credential (identity = пользователь) + ключ подписи (устройство).
pub struct MlsIdentity {
    pub provider: Provider,
    pub signer: SignatureKeyPair,
    pub credential_with_key: CredentialWithKey,
}

/// Создать личность. `identity` — байты аккаунта (для фазы A — публичный ключ).
pub fn new_identity(identity: &[u8]) -> Result<MlsIdentity, MlsError> {
    let provider = OpenMlsRustCrypto::default();
    let signer = mls(SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()))?;
    mls(signer.store(provider.storage()))?;
    let credential = BasicCredential::new(identity.to_vec());
    let credential_with_key = CredentialWithKey {
        credential: credential.into(),
        signature_key: signer.public().into(),
    };
    let id = MlsIdentity { provider, signer, credential_with_key };
    persist::remember_signer(&id);
    Ok(id)
}

/// Создать личность из готовых ключей: `account_identity` — байты аккаунта
/// (credential), `private`/`public` — ключ подписи устройства (Ed25519, 32+32).
pub fn from_parts(
    account_identity: &[u8],
    private: &[u8],
    public: &[u8],
) -> Result<MlsIdentity, MlsError> {
    let provider = OpenMlsRustCrypto::default();
    let signer = SignatureKeyPair::from_raw(
        CIPHERSUITE.signature_algorithm(),
        private.to_vec(),
        public.to_vec(),
    );
    mls(signer.store(provider.storage()))?;
    let credential = BasicCredential::new(account_identity.to_vec());
    let credential_with_key = CredentialWithKey {
        credential: credential.into(),
        signature_key: signer.public().into(),
    };
    let id = MlsIdentity { provider, signer, credential_with_key };
    persist::remember_signer(&id);
    Ok(id)
}

/// Сгенерировать и сериализовать KeyPackage (публичная часть для впуска).
pub fn key_package(id: &MlsIdentity) -> Result<Vec<u8>, MlsError> {
    let bundle = mls(KeyPackage::builder().build(
        CIPHERSUITE,
        &id.provider,
        &id.signer,
        id.credential_with_key.clone(),
    ))?;
    codec(bundle.key_package().tls_serialize_detached())
}

/// Создать группу с заданным id (наш group_id).
pub fn create_group(id: &MlsIdentity, group_id: &[u8]) -> Result<MlsGroup, MlsError> {
    let config = MlsGroupCreateConfig::builder()
        .ciphersuite(CIPHERSUITE)
        .use_ratchet_tree_extension(true)
        .build();
    mls(MlsGroup::new_with_group_id(
        &id.provider,
        &id.signer,
        &config,
        GroupId::from_slice(group_id),
        id.credential_with_key.clone(),
    ))
}

/// Добавить участника: вернуть `(commit, welcome)` и продвинуть свою эпоху.
pub fn add_member(
    group: &mut MlsGroup,
    id: &MlsIdentity,
    key_package_bytes: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), MlsError> {
    let kp_in = codec(KeyPackageIn::tls_deserialize_exact(key_package_bytes))?;
    let kp = mls(kp_in.validate(id.provider.crypto(), ProtocolVersion::Mls10))?;
    let (commit, welcome, _group_info) =
        mls(group.add_members(&id.provider, &id.signer, std::slice::from_ref(&kp)))?;
    mls(group.merge_pending_commit(&id.provider))?;
    Ok((codec(commit.tls_serialize_detached())?, codec(welcome.tls_serialize_detached())?))
}

/// Войти в группу по Welcome.
pub fn join(id: &MlsIdentity, welcome_bytes: &[u8]) -> Result<MlsGroup, MlsError> {
    let msg = codec(MlsMessageIn::tls_deserialize_exact(welcome_bytes))?;
    let welcome = match msg.extract() {
        MlsMessageBodyIn::Welcome(w) => w,
        _ => return Err(MlsError::NotWelcome),
    };
    let staged = mls(StagedWelcome::new_from_welcome(
        &id.provider,
        &MlsGroupJoinConfig::default(),
        welcome,
        None,
    ))?;
    mls(staged.into_group(&id.provider))
}

/// Зашифровать прикладное сообщение (наша полезная нагрузка).
pub fn encrypt(
    group: &mut MlsGroup,
    id: &MlsIdentity,
    plaintext: &[u8],
) -> Result<Vec<u8>, MlsError> {
    let out = mls(group.create_message(&id.provider, &id.signer, plaintext))?;
    codec(out.tls_serialize_detached())
}

/// Расшифровать сообщение. Возвращает `None` для не-прикладных (Commit/Proposal).
pub fn decrypt(
    group: &mut MlsGroup,
    id: &MlsIdentity,
    bytes: &[u8],
) -> Result<Option<Vec<u8>>, MlsError> {
    Ok(match process(group, id, bytes)? {
        Received::Application(app) => Some(app),
        _ => None,
    })
}

/// Что пришло в сообщении (после обработки).
#[derive(Debug, PartialEq, Eq)]
pub enum Received {
    /// Прикладные данные (наша полезная нагрузка).
    Application(Vec<u8>),
    /// Handshake (Commit/Proposal) — уже применён к группе.
    Handshake,
    /// Прочее (например, standalone-предложение).
    Other,
}

/// Обработать входящее MLS-сообщение. Commit'ы применяются (merge) автоматически.
pub fn process(
    group: &mut MlsGroup,
    id: &MlsIdentity,
    bytes: &[u8],
) -> Result<Received, MlsError> {
    let msg = codec(MlsMessageIn::tls_deserialize_exact(bytes))?;
    let protocol = mls(msg.try_into_protocol_message())?;
    let processed = mls(group.process_message(&id.provider, protocol))?;
    Ok(match processed.into_content() {
        ProcessedMessageContent::ApplicationMessage(app) => Received::Application(app.into_bytes()),
        ProcessedMessageContent::StagedCommitMessage(staged) => {
            mls(group.merge_staged_commit(&id.provider, *staged))?;
            Received::Handshake
        }
        _ => Received::Other,
    })
}

/// Удалить участника по identity (credential). Возвращает commit для остальных.
/// Свою эпоху продвигаем сразу (merge_pending_commit).
pub fn remove_member(
    group: &mut MlsGroup,
    id: &MlsIdentity,
    identity: &[u8],
) -> Result<Vec<u8>, MlsError> {
    let index = group
        .members()
        .find(|m| m.credential.serialized_content() == identity)
        .map(|m| m.index)
        .ok_or_else(|| MlsError::Mls("участник не найден".into()))?;
    let (commit, _welcome, _gi) = mls(group.remove_members(&id.provider, &id.signer, &[index]))?;
    mls(group.merge_pending_commit(&id.provider))?;
    codec(commit.tls_serialize_detached())
}

/// Число участников группы.
pub fn member_count(group: &MlsGroup) -> usize {
    group.members().count()
}

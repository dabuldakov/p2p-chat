//! Мост между нашей личностью (`crypto::Identity`, Ed25519) и MLS.
//!
//! `credential.identity` = публичный ключ аккаунта; ключ подписи устройства =
//! та же Ed25519-личность (фаза A: одно устройство на аккаунт). На этапе B
//! сюда придёт отдельный device-ключ и сертификат устройства.

use p2pchat_crypto::Identity;
use p2pchat_mls::{from_parts, MlsError, MlsIdentity};

use std::collections::HashMap;

use crate::{CoreError, GroupId};

/// Реэкспорт результата обработки MLS-сообщения.
pub use p2pchat_mls::Received;

/// Построить MLS-личность из нашей.
pub fn identity_from(id: &Identity) -> Result<MlsIdentity, MlsError> {
    let account = id.public_key().to_bytes();
    let private = id.secret_bytes();
    from_parts(&account, &private, &account)
}

/// Набор MLS-групп узла: личность (аккаунт + устройство) и открытые состояния.
pub struct MlsGroups {
    id: MlsIdentity,
    groups: HashMap<GroupId, p2pchat_mls::MlsGroup>,
}

impl MlsGroups {
    pub fn from_crypto(id: &Identity) -> Result<Self, CoreError> {
        Ok(Self { id: identity_from(id)?, groups: HashMap::new() })
    }

    /// Наш KeyPackage для впуска в чужие группы.
    pub fn key_package(&self) -> Result<Vec<u8>, CoreError> {
        Ok(p2pchat_mls::key_package(&self.id)?)
    }

    pub fn create(&mut self, group_id: &[u8]) -> Result<(), CoreError> {
        let group = p2pchat_mls::create_group(&self.id, group_id)?;
        self.groups.insert(group_id.to_vec(), group);
        Ok(())
    }

    pub fn has(&self, group_id: &[u8]) -> bool {
        self.groups.contains_key(group_id)
    }

    /// Убрать группу из локального состояния (выход).
    pub fn remove(&mut self, group_id: &[u8]) -> bool {
        self.groups.remove(group_id).is_some()
    }

    /// id всех известных групп.
    pub fn group_ids(&self) -> Vec<GroupId> {
        self.groups.keys().cloned().collect()
    }

    /// Добавить участника по его KeyPackage: вернуть `(commit, welcome)`.
    pub fn add_member(
        &mut self,
        group_id: &[u8],
        key_package: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), CoreError> {
        let Self { id, groups } = self;
        let group = groups
            .get_mut(group_id)
            .ok_or_else(|| CoreError::BadEnvelope("mls: неизвестная группа".into()))?;
        Ok(p2pchat_mls::add_member(group, id, key_package)?)
    }

    /// Войти по Welcome: вернуть id группы.
    pub fn join(&mut self, welcome: &[u8]) -> Result<GroupId, CoreError> {
        let group = p2pchat_mls::join(&self.id, welcome)?;
        let gid = p2pchat_mls::group_id(&group);
        self.groups.insert(gid.clone(), group);
        Ok(gid)
    }

    pub fn encrypt(&mut self, group_id: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, CoreError> {
        let Self { id, groups } = self;
        let group = groups
            .get_mut(group_id)
            .ok_or_else(|| CoreError::BadEnvelope("mls: неизвестная группа".into()))?;
        Ok(p2pchat_mls::encrypt(group, id, plaintext)?)
    }

    pub fn process(&mut self, group_id: &[u8], bytes: &[u8]) -> Result<Received, CoreError> {
        let Self { id, groups } = self;
        let group = groups
            .get_mut(group_id)
            .ok_or_else(|| CoreError::BadEnvelope("mls: неизвестная группа".into()))?;
        Ok(p2pchat_mls::process(group, id, bytes)?)
    }

    /// Удалить участника по identity (account pubkey): вернуть commit.
    pub fn remove_member(
        &mut self,
        group_id: &[u8],
        identity: &[u8],
    ) -> Result<Vec<u8>, CoreError> {
        let Self { id, groups } = self;
        let group = groups
            .get_mut(group_id)
            .ok_or_else(|| CoreError::BadEnvelope("mls: неизвестная группа".into()))?;
        Ok(p2pchat_mls::remove_member(group, id, identity)?)
    }

    /// Идентичности (account pubkey) участников группы.
    pub fn members(&self, group_id: &[u8]) -> Vec<Vec<u8>> {
        self.groups
            .get(group_id)
            .map(p2pchat_mls::members)
            .unwrap_or_default()
    }

    /// Сохранить состояние (снапшот KV провайдера) в SQLite.
    pub fn save(&self, db_path: &str) -> Result<(), CoreError> {
        Ok(p2pchat_mls::save(&self.id, db_path)?)
    }

    /// Поднять состояние и личность из SQLite (или создать личность, если снапшота нет).
    pub fn load(account: &Identity, db_path: &str) -> Result<Self, CoreError> {
        let account_bytes = account.public_key().to_bytes();
        let id = match p2pchat_mls::load_identity(&account_bytes, db_path) {
            Ok(id) => id,
            Err(_) => identity_from(account)?,
        };
        Ok(Self { id, groups: HashMap::new() })
    }

    /// Поднять одну группу из сохранённого состояния в карту.
    pub fn load_group(&mut self, group_id: &[u8]) -> Result<bool, CoreError> {
        if self.groups.contains_key(group_id) {
            return Ok(true);
        }
        match p2pchat_mls::load_group(&self.id, group_id)? {
            Some(g) => {
                self.groups.insert(group_id.to_vec(), g);
                Ok(true)
            }
            None => Ok(false),
        }
    }
}


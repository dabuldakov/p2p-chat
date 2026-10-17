//! Персистентность OpenMLS: снапшот key-value хранилища провайдера в SQLite.
//!
//! OpenMLS хранит всё состояние (эпохи, секреты, ключи подписи, состояние групп)
//! в key-value хранилище провайдера ([`Provider::storage`], `MemoryStorage`).
//! Мы сохраняем этот KV целиком в таблицу `mls_kv` атомарной транзакцией после
//! операций и восстанавливаем при старте. Группа поднимается через
//! [`MlsGroup::load`].

use std::path::Path;

use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use rusqlite::{params, Connection};

use crate::{MlsError, MlsIdentity, CIPHERSUITE};

/// Ключ в KV, под которым мы храним публичный ключ подписи (устройства),
/// чтобы восстановить `SignatureKeyPair` после перезапуска.
const SIGNER_PUB_KEY: &[u8] = b"p2pchat:signer_pub";

fn sql<E: std::fmt::Display>(e: E) -> MlsError {
    MlsError::Mls(format!("sqlite: {e}"))
}

/// Сохранить состояние провайдера (весь KV) в SQLite.
pub fn save(id: &MlsIdentity, db_path: &str) -> Result<(), MlsError> {
    let conn = Connection::open(db_path).map_err(sql)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS mls_kv (key BLOB PRIMARY KEY, value BLOB NOT NULL);",
    )
    .map_err(sql)?;
    let tx = conn.unchecked_transaction().map_err(sql)?;
    tx.execute("DELETE FROM mls_kv", []).map_err(sql)?;
    {
        let values = id.provider.storage().values.read().unwrap();
        let mut stmt = tx
            .prepare("INSERT INTO mls_kv (key, value) VALUES (?1, ?2)")
            .map_err(sql)?;
        for (k, v) in values.iter() {
            stmt.execute(params![k, v]).map_err(sql)?;
        }
    }
    tx.commit().map_err(sql)?;
    Ok(())
}

/// Восстановить состояние провайдера из SQLite и собрать личность заново.
pub fn load_identity(identity: &[u8], db_path: &str) -> Result<MlsIdentity, MlsError> {
    let provider = openmls_rust_crypto::OpenMlsRustCrypto::default();

    if Path::new(db_path).exists() {
        let conn = Connection::open(db_path).map_err(sql)?;
        let rows: Vec<(Vec<u8>, Vec<u8>)> = {
            let mut stmt = match conn.prepare("SELECT key, value FROM mls_kv") {
                Ok(s) => s,
                Err(_) => {
                    // Таблицы ещё нет — пустое хранилище.
                    return finish_identity(provider, identity);
                }
            };
            let it = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(sql)?;
            it.collect::<Result<_, _>>().map_err(sql)?
        };
        let mut values = provider.storage().values.write().unwrap();
        for (k, v) in rows {
            values.insert(k, v);
        }
    }

    finish_identity(provider, identity)
}

fn finish_identity(
    provider: openmls_rust_crypto::OpenMlsRustCrypto,
    identity: &[u8],
) -> Result<MlsIdentity, MlsError> {
    let public = provider
        .storage()
        .values
        .read()
        .unwrap()
        .get(SIGNER_PUB_KEY)
        .cloned()
        .ok_or_else(|| MlsError::Mls("signer pubkey не найден в хранилище".into()))?;
    let signer = SignatureKeyPair::read(provider.storage(), &public, CIPHERSUITE.signature_algorithm())
        .ok_or_else(|| MlsError::Mls("SignatureKeyPair не восстановлен".into()))?;
    let credential = BasicCredential::new(identity.to_vec());
    let credential_with_key = CredentialWithKey {
        credential: credential.into(),
        signature_key: signer.public().into(),
    };
    Ok(MlsIdentity { provider, signer, credential_with_key })
}

/// Записать публичный ключ подписи в KV (вызывается при создании личности).
pub(crate) fn remember_signer(id: &MlsIdentity) {
    id.provider
        .storage()
        .values
        .write()
        .unwrap()
        .insert(SIGNER_PUB_KEY.to_vec(), id.signer.public().to_vec());
}

/// Поднять группу из сохранённого состояния.
pub fn load_group(id: &MlsIdentity, group_id: &[u8]) -> Result<Option<MlsGroup>, MlsError> {
    MlsGroup::load(id.provider.storage(), &GroupId::from_slice(group_id))
        .map_err(|e| MlsError::Mls(format!("{e:?}")))
}

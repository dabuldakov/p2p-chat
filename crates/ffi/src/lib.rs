//! UniFFI-мост: тонкая обёртка над `core` для Kotlin/Android.
//!
//! Из Rust наружу торчит минимум: создать ядро, отдать/принять байты,
//! получить историю. Вся работа с WebRTC и UI — на стороне Kotlin.
//!
//! Генерация Kotlin-биндингов:
//! `cargo run -p p2pchat-ffi --bin uniffi-bindgen generate ...`
//! (добавим позже; сейчас важна компилируемость и форма API).

uniffi::setup_scaffolding!();

use std::sync::{Arc, Mutex};

use p2pchat_core::{GossipEngine, GroupId, Log};

/// Срок жизни инвайт-билета по умолчанию (сутки).
const INVITE_TTL_SECS: u64 = 24 * 3600;

/// Ошибка на границе FFI (UniFFI сам превратит в Kotlin-исключение).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiError {
    #[error("{reason}")]
    Core { reason: String },
}

impl From<p2pchat_core::CoreError> for FfiError {
    fn from(e: p2pchat_core::CoreError) -> Self {
        FfiError::Core {
            reason: e.to_string(),
        }
    }
}

/// Одно сообщение для UI.
#[derive(uniffi::Record)]
pub struct FfiMessage {
    pub id: Vec<u8>,
    pub group_id: Vec<u8>,
    pub author: Vec<u8>,
    pub text: String,
    pub lamport: u64,
    pub wall_time: u64,
    pub edited: bool,
    pub deleted: bool,
    pub attachment: Option<FfiAttachment>,
    pub reactions: Vec<FfiReaction>,
}

/// Реакция на сообщение (агрегированная).
#[derive(uniffi::Record)]
pub struct FfiReaction {
    pub emoji: String,
    pub count: u32,
    pub mine: bool,
}

/// Вложение-ссылка (гифка). Байты медиа не передаются — клиент тянет их сам.
#[derive(uniffi::Record)]
pub struct FfiAttachment {
    pub kind: String,
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub url: String,
    pub mime: String,
    pub size: u64,
    pub duration_ms: u64,
    pub file_name: String,
    pub thumbnail: Vec<u8>,
}

/// Подписанная жалоба для серверного endpoint.
#[derive(uniffi::Record)]
pub struct FfiReport {
    pub payload: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Вход для публикации своих связей: с кем и как подключены.
#[derive(uniffi::Record)]
pub struct FfiPeerLink {
    pub peer_id: Vec<u8>,
    pub kind: String,
}

/// Аватар участника.
#[derive(uniffi::Record)]
pub struct FfiAvatar {
    pub peer_id: Vec<u8>,
    pub avatar: Vec<u8>,
}

/// Связь для графа: автор → собеседник и тип ("direct"/"turn").
#[derive(uniffi::Record)]
pub struct FfiLink {
    pub from: Vec<u8>,
    pub to: Vec<u8>,
    pub kind: String,
}

/// Пир для UI.
#[derive(uniffi::Record)]
pub struct FfiPeer {
    pub peer_id: Vec<u8>,
    pub last_seen: u64,
    pub is_relay: bool,
}

/// Участник группы.
#[derive(uniffi::Record)]
pub struct FfiMember {
    pub peer_id: Vec<u8>,
    pub public_key: Vec<u8>,
}

/// Заявка на владение в активных выборах.
#[derive(uniffi::Record)]
pub struct FfiOwnerClaim {
    pub claim_id: Vec<u8>,
    pub claimant: Vec<u8>,
    pub trigger: u32,
    pub created_at: u64,
}

/// Голос в активных выборах.
#[derive(uniffi::Record)]
pub struct FfiOwnerVote {
    pub claim_id: Vec<u8>,
    pub voter: Vec<u8>,
    pub approve: bool,
    pub keep: bool,
}

/// Активные выборы владельца.
#[derive(uniffi::Record)]
pub struct FfiOwnerElection {
    pub trigger: u32,
    pub created_at: u64,
    pub vote_start: u64,
    pub ends_at: u64,
    pub claims: Vec<FfiOwnerClaim>,
    pub votes: Vec<FfiOwnerVote>,
}

/// Конверт для отправки с адресатами (публичные ключи).
/// Пустой `targets` — разослать всем подключённым.
#[derive(uniffi::Record)]
pub struct FfiOutgoing {
    pub envelope: Vec<u8>,
    pub targets: Vec<Vec<u8>>,
}

/// Результат вступления по инвайту: id группы и публичный ключ приглашающего
/// (к нему подключаемся для bootstrap).
#[derive(uniffi::Record)]
pub struct FfiJoin {
    pub group_id: Vec<u8>,
    pub host_key: Vec<u8>,
    /// Заявка на впуск (KeyPackage + билет) — отправить хозяину после подключения.
    pub request: FfiOutgoing,
}

/// Страница истории: сообщения (по возрастанию) и есть ли ещё более старые.
#[derive(uniffi::Record)]
pub struct FfiHistoryPage {
    pub messages: Vec<FfiMessage>,
    pub has_more: bool,
}

/// Никнейм участника.
#[derive(uniffi::Record)]
pub struct FfiNickname {
    pub peer_id: Vec<u8>,
    pub nickname: String,
}

/// Ядро чата, которым владеет Android-слой.
#[derive(uniffi::Object)]
pub struct ChatCore {
    inner: Mutex<GossipEngine>,
    identity_path: Option<std::path::PathBuf>,
}

#[uniffi::export]
impl ChatCore {
    /// Создать ядро. `db_path` — файл SQLite (или ":memory:").
    /// Личность хранится отдельным файлом рядом с БД (`identity.key`),
    /// чтобы эксперименты со схемой БД не сбрасывали аккаунт.
    #[uniffi::constructor]
    pub fn new(db_path: String) -> Result<Arc<Self>, FfiError> {
        let store = if db_path == ":memory:" {
            p2pchat_storage::Store::open_in_memory()
        } else {
            p2pchat_storage::Store::open(&db_path)
        }
        .map_err(|e| FfiError::Core {
            reason: e.to_string(),
        })?;

        let identity_path = identity_path_for(&db_path);
        let identity = load_or_create_identity(identity_path.as_deref(), &store)?;

        let log = Log::with_store(store)?;
        let mut engine = GossipEngine::new(identity, log);
        engine.load_state()?;
        engine.load_mls(&db_path)?;
        Ok(Arc::new(Self {
            inner: Mutex::new(engine),
            identity_path,
        }))
    }

    /// Наш peer_id (hex-строкой — удобно для UI и логов).
    pub fn peer_id_hex(&self) -> String {
        hex(&self.inner.lock().unwrap().peer_id())
    }

    /// Наш публичный ключ (его передаём собеседникам).
    pub fn public_key(&self) -> Vec<u8> {
        self.inner.lock().unwrap().public_key()
    }

    /// Собрать и подписать жалобу для сервера.
    #[allow(clippy::too_many_arguments)]
    pub fn create_report(
        &self,
        reported_peer: Vec<u8>,
        group_id: Vec<u8>,
        message_id: Vec<u8>,
        reason: String,
        note: String,
        screenshot: Vec<u8>,
    ) -> Result<FfiReport, FfiError> {
        let engine = self.inner.lock().unwrap();
        let mut nonce = vec![0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
        let report = p2pchat_core::proto::Report {
            reporter_key: engine.public_key(),
            reported_peer,
            group_id,
            message_id,
            reason,
            note,
            screenshot,
            timestamp: p2pchat_core::unix_now(),
            nonce,
        };
        use prost::Message as _;
        let payload = report.encode_to_vec();
        let signature = engine.sign(&payload);
        Ok(FfiReport { payload, signature })
    }

    /// Отправить heartbeat владельца (только владелец).
    pub fn owner_alive(&self, group_id: Vec<u8>) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine.owner_alive(&group_id)?.map(|o| FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        }))
    }

    /// Когда владелец последний раз был активен (unix seconds; 0 — нет данных).
    pub fn owner_last_seen(&self, group_id: Vec<u8>) -> u64 {
        self.inner.lock().unwrap().owner_last_seen(&group_id)
    }

    /// Номер эпохи владельца группы.
    pub fn owner_epoch(&self, group_id: Vec<u8>) -> u64 {
        self.inner.lock().unwrap().owner_epoch(&group_id)
    }

    /// Когда последний раз шёл вызов владельца (unix seconds; 0 — не было).
    pub fn owner_challenge_at(&self, group_id: Vec<u8>) -> u64 {
        self.inner.lock().unwrap().owner_challenge_at(&group_id)
    }

    /// Активные выборы владельца (для карточки в UI).
    pub fn owner_election(&self, group_id: Vec<u8>) -> Option<FfiOwnerElection> {
        let e = self.inner.lock().unwrap().owner_election(&group_id)?;
        Some(FfiOwnerElection {
            trigger: e.trigger as u32,
            created_at: e.created_at,
            vote_start: e.vote_start,
            ends_at: e.ends_at,
            claims: e
                .claims
                .into_iter()
                .map(|c| FfiOwnerClaim {
                    claim_id: c.claim_id,
                    claimant: c.claimant,
                    trigger: c.trigger as u32,
                    created_at: c.created_at,
                })
                .collect(),
            votes: e
                .votes
                .into_iter()
                .map(|v| FfiOwnerVote {
                    claim_id: v.claim_id,
                    voter: v.voter,
                    approve: v.approve,
                    keep: v.keep,
                })
                .collect(),
        })
    }

    /// Подать заявку на владение: 0 — отсутствие, 1 — вызов, 2 — авто-преемник.
    pub fn create_owner_claim(
        &self,
        group_id: Vec<u8>,
        trigger: u32,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        use p2pchat_core::proto::OwnerTrigger;
        let trigger = match trigger {
            0 => OwnerTrigger::Absent,
            1 => OwnerTrigger::Challenge,
            2 => OwnerTrigger::Auto,
            _ => {
                return Err(FfiError::Core {
                    reason: "unknown owner trigger".into(),
                })
            }
        };
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .create_owner_claim(&group_id, trigger)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Голос: за заявку (`approve`) или оставить владельца (`keep`).
    pub fn owner_vote(
        &self,
        group_id: Vec<u8>,
        claim_id: Vec<u8>,
        approve: bool,
        keep: bool,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .owner_vote(&group_id, &claim_id, approve, keep)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Завершить выборы, если окно истекло.
    pub fn finalize_owner_election(
        &self,
        group_id: Vec<u8>,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .finalize_owner_election(&group_id)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Последний OwnerChange для догона офлайн-участников.
    pub fn owner_change_envelope(
        &self,
        group_id: Vec<u8>,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .owner_change_envelope(&group_id)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Поставить/снять реакцию на сообщение.
    pub fn react(
        &self,
        group_id: Vec<u8>,
        target_id: Vec<u8>,
        emoji: String,
        add: bool,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .react(&group_id, &target_id, &emoji, add)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Заблокировать пользователя (локально).
    pub fn block_peer(&self, peer_id: Vec<u8>) {
        self.inner.lock().unwrap().block_user(&peer_id);
    }

    /// Разблокировать пользователя.
    pub fn unblock_peer(&self, peer_id: Vec<u8>) {
        self.inner.lock().unwrap().unblock_user(&peer_id);
    }

    /// Список заблокированных peer_id.
    pub fn blocked_peers(&self) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().blocked_users()
    }

    /// Короткий отпечаток публичного ключа для сверки «тот ли это человек».
    pub fn fingerprint(&self) -> String {
        let id = self.inner.lock().unwrap().peer_id();
        let s: String = id[..8].iter().map(|b| format!("{b:02X}")).collect();
        s.as_bytes()
            .chunks(4)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Экспорт seed-фразы (24 слова, BIP39) текущей идентичности.
    pub fn export_seed(&self) -> Result<String, FfiError> {
        let secret = self.inner.lock().unwrap().secret_bytes();
        let m = bip39::Mnemonic::from_entropy(&secret).map_err(|e| FfiError::Core {
            reason: e.to_string(),
        })?;
        Ok(m.to_string())
    }

    /// Восстановить идентичность из seed-фразы. Только на пустом профиле
    /// (без групп) — иначе вернёт ошибку. Возвращает новый peer_id (hex).
    pub fn import_seed(&self, phrase: String) -> Result<String, FfiError> {
        let m = bip39::Mnemonic::parse(phrase.trim()).map_err(|e| FfiError::Core {
            reason: e.to_string(),
        })?;
        let entropy = m.to_entropy();
        if entropy.len() != 32 {
            return Err(FfiError::Core {
                reason: "нужна seed-фраза из 24 слов".into(),
            });
        }
        let identity =
            p2pchat_crypto::Identity::from_secret_bytes(&entropy).map_err(|e| FfiError::Core {
                reason: e.to_string(),
            })?;
        let mut engine = self.inner.lock().unwrap();
        if !engine.groups().is_empty() {
            return Err(FfiError::Core {
                reason: "сначала сбросьте данные приложения".into(),
            });
        }
        if let Some(p) = &self.identity_path {
            write_atomic(p, &identity.secret_bytes()).map_err(io_err)?;
        }
        engine.replace_identity(identity)?;
        Ok(hex(&engine.peer_id()))
    }

    /// Отправить сообщение в группу. Возвращает конверт с адресатами.
    pub fn post(&self, group_id: Vec<u8>, text: String) -> Result<FfiOutgoing, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let o = engine.post(group_id, text)?;
        use prost::Message as _;
        Ok(FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        })
    }

    /// Отправить вложение (гифка, фото, видео, аудио, голос).
    pub fn post_attachment(
        &self,
        group_id: Vec<u8>,
        attachment: FfiAttachment,
    ) -> Result<FfiOutgoing, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let o = engine.post_attachment(
            group_id,
            p2pchat_core::proto::Attachment {
                kind: attachment.kind,
                id: attachment.id,
                width: attachment.width,
                height: attachment.height,
                url: attachment.url,
                mime: attachment.mime,
                size: attachment.size,
                duration_ms: attachment.duration_ms,
                file_name: attachment.file_name,
                thumbnail: attachment.thumbnail,
            },
        )?;
        use prost::Message as _;
        Ok(FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        })
    }

    /// Совместимый алиас для GIF.
    pub fn post_gif(
        &self,
        group_id: Vec<u8>,
        attachment: FfiAttachment,
    ) -> Result<FfiOutgoing, FfiError> {
        self.post_attachment(group_id, attachment)
    }

    /// Отредактировать своё сообщение (только автор). Возвращает конверт.
    pub fn edit_message(
        &self,
        group_id: Vec<u8>,
        target_id: Vec<u8>,
        text: String,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .post_edit(&group_id, &target_id, text)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Удалить своё сообщение (только автор). Возвращает конверт.
    pub fn delete_message(
        &self,
        group_id: Vec<u8>,
        target_id: Vec<u8>,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .post_delete(&group_id, &target_id)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Гифки, которые отправлял я (вкладка «Мои»), свежие сверху.
    pub fn recent_gifs(&self, limit: u32) -> Vec<FfiAttachment> {
        let engine = self.inner.lock().unwrap();
        engine
            .log()
            .gifs_by_author(&engine.peer_id(), limit as i64)
            .unwrap_or_default()
            .into_iter()
            .map(to_ffi_attachment)
            .collect()
    }

    /// Гифки текущей группы (вкладка «Эта группа»), свежие сверху.
    pub fn group_gifs(&self, group_id: Vec<u8>, limit: u32) -> Vec<FfiAttachment> {
        let engine = self.inner.lock().unwrap();
        engine
            .log()
            .gifs_for_group(&group_id, limit as i64)
            .unwrap_or_default()
            .into_iter()
            .map(to_ffi_attachment)
            .collect()
    }

    /// Создать группу (MLS). Ключ раздаётся участникам попарно при впуске.
    pub fn create_group(&self, group_id: Vec<u8>) -> Result<(), FfiError> {
        let mut engine = self.inner.lock().unwrap();
        engine.create_group(group_id)?;
        Ok(())
    }

    /// Создать группу со случайным id и отдельным именем. Возвращает id.
    pub fn create_named_group(&self, name: String) -> Result<Vec<u8>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        Ok(engine.create_named_group(&name)?)
    }

    /// Отображаемое имя группы (fallback — строка id для старых групп).
    pub fn group_name(&self, group_id: Vec<u8>) -> String {
        self.inner.lock().unwrap().group_name(&group_id)
    }

    /// Создать группу и сразу вернуть одноразовый инвайт-билет (ссылка/QR).
    pub fn create_group_invite(&self, group_id: Vec<u8>) -> Result<String, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        engine.create_group(group_id.clone())?;
        Ok(engine.create_invite(&group_id, INVITE_TTL_SECS, 1)?)
    }

    /// Инвайт-билет существующей группы (только владелец).
    /// `max_uses` — сколько раз можно войти (1 — одноразовый), срок — сутки.
    pub fn group_invite(
        &self,
        group_id: Vec<u8>,
        max_uses: u32,
    ) -> Result<Option<String>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        Ok(engine
            .create_invite(&group_id, INVITE_TTL_SECS, max_uses)
            .ok())
    }

    /// Начать впуск по билету: id группы, ключ хозяина и заявка для отправки.
    pub fn join_from_invite(&self, invite: String) -> Result<FfiJoin, FfiError> {
        let engine = self.inner.lock().unwrap();
        let (group_id, o) = engine.join_request(&invite)?;
        use prost::Message as _;
        let host_key = o.targets.first().cloned().unwrap_or_default();
        let request = FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        };
        Ok(FfiJoin {
            group_id,
            host_key,
            request,
        })
    }

    /// Список групп, в которых мы состоим.
    pub fn list_groups(&self) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().groups()
    }

    /// Отметить группу прочитанной (до последнего сообщения).
    pub fn mark_read(&self, group_id: Vec<u8>) -> Result<(), FfiError> {
        self.inner.lock().unwrap().mark_read(&group_id)?;
        Ok(())
    }

    /// Число непрочитанных сообщений в группе.
    pub fn unread_count(&self, group_id: Vec<u8>) -> Result<u64, FfiError> {
        Ok(self.inner.lock().unwrap().unread_count(&group_id)?)
    }

    /// Выйти из группы. Возвращает конверт для рассылки (или None).
    pub fn leave_group(&self, group_id: Vec<u8>) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine.leave_group(&group_id)?.map(|o| FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        }))
    }

    /// Удалить участника (только владелец). Возвращает конверты для рассылки.
    pub fn remove_member(
        &self,
        group_id: Vec<u8>,
        peer_id: Vec<u8>,
    ) -> Result<Vec<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .remove_member(&group_id, &peer_id)?
            .into_iter()
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            })
            .collect())
    }

    /// Являемся ли мы владельцем группы.
    pub fn is_owner(&self, group_id: Vec<u8>) -> Result<bool, FfiError> {
        Ok(self.inner.lock().unwrap().is_owner(&group_id)?)
    }

    /// peer_id владельца группы (для звезды в списке участников).
    pub fn owner_id(&self, group_id: Vec<u8>) -> Option<Vec<u8>> {
        self.inner.lock().unwrap().owner_of(&group_id)
    }

    /// Задать свой никнейм; возвращает конверт для рассылки.
    pub fn set_nickname(&self, name: String) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine.set_nickname(&name)?.map(|o| FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        }))
    }

    /// Задать свой аватар (маленькое изображение) и вернуть конверт для рассылки.
    pub fn set_avatar(&self, avatar: Vec<u8>) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine.set_avatar(avatar)?.map(|o| FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        }))
    }

    /// Аватар участника (или None).
    pub fn avatar_of(&self, peer_id: Vec<u8>) -> Option<Vec<u8>> {
        self.inner.lock().unwrap().avatar_of(&peer_id)
    }

    /// Все аватары участников.
    pub fn avatars(&self) -> Vec<FfiAvatar> {
        self.inner
            .lock()
            .unwrap()
            .avatars()
            .into_iter()
            .map(|(peer_id, avatar)| FfiAvatar { peer_id, avatar })
            .collect()
    }

    /// Сменить аватар группы (только владелец). Возвращает конверт для рассылки.
    pub fn set_group_avatar(
        &self,
        group_id: Vec<u8>,
        avatar: Vec<u8>,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .set_group_avatar(&group_id, avatar)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Переименовать группу (только владелец). Возвращает конверт.
    pub fn rename_group(
        &self,
        group_id: Vec<u8>,
        name: String,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine
            .set_group_name(&group_id, name)?
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            }))
    }

    /// Аватар группы (или None).
    pub fn group_avatar(&self, group_id: Vec<u8>) -> Option<Vec<u8>> {
        self.inner.lock().unwrap().group_avatar(&group_id)
    }

    /// Конверт с нашим профилем для повторной рассылки (если ник задан).
    pub fn profile_envelope(&self) -> Result<Option<FfiOutgoing>, FfiError> {
        let engine = self.inner.lock().unwrap();
        use prost::Message as _;
        Ok(engine.profile_envelope()?.map(|o| FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        }))
    }

    pub fn my_nickname(&self) -> Option<String> {
        self.inner.lock().unwrap().my_nickname()
    }

    pub fn nickname_of(&self, peer_id: Vec<u8>) -> Option<String> {
        self.inner.lock().unwrap().nickname_of(&peer_id)
    }

    pub fn all_nicknames(&self) -> Vec<FfiNickname> {
        self.inner
            .lock()
            .unwrap()
            .all_nicknames()
            .into_iter()
            .map(|(peer_id, nickname)| FfiNickname { peer_id, nickname })
            .collect()
    }

    /// Участники группы.
    pub fn members(&self, group_id: Vec<u8>) -> Vec<FfiMember> {
        self.inner
            .lock()
            .unwrap()
            .members(&group_id)
            .into_iter()
            .map(|m| FfiMember {
                peer_id: m.peer_id,
                public_key: m.public_key,
            })
            .collect()
    }

    /// Соседи группы (peer_id).
    pub fn neighbors(&self, group_id: Vec<u8>) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().neighbors(&group_id)
    }

    /// Публичные ключи соседей группы (кому соединяться/пересылать).
    pub fn neighbor_keys(&self, group_id: Vec<u8>) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().neighbor_keys(&group_id)
    }

    /// Опубликовать свои активные связи в группе (эфемерно). Вернуть конверт.
    pub fn post_links(
        &self,
        group_id: Vec<u8>,
        links: Vec<FfiPeerLink>,
    ) -> Result<Option<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let ls = links.into_iter().map(|l| (l.peer_id, l.kind)).collect();
        use prost::Message as _;
        Ok(engine.post_links(&group_id, ls)?.map(|o| FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        }))
    }

    /// Снимок связей группы для графа: (from, to, kind).
    pub fn links(&self, group_id: Vec<u8>) -> Vec<FfiLink> {
        self.inner
            .lock()
            .unwrap()
            .links(&group_id)
            .into_iter()
            .map(|(from, to, kind)| FfiLink { from, to, kind })
            .collect()
    }

    /// Группы, где мы состоим и есть участник с таким публичным ключом.
    pub fn groups_with(&self, public_key: Vec<u8>) -> Vec<Vec<u8>> {
        self.inner.lock().unwrap().groups_with(&public_key)
    }

    /// Merkle-digest группы, адресованный конкретному пиру (офлайн-догон).
    pub fn sync_digest_to(
        &self,
        group_id: Vec<u8>,
        target_key: Vec<u8>,
    ) -> Result<FfiOutgoing, FfiError> {
        let engine = self.inner.lock().unwrap();
        let o = engine.sync_digest_to(&group_id, &target_key)?;
        use prost::Message as _;
        Ok(FfiOutgoing {
            envelope: o.envelope.encode_to_vec(),
            targets: o.targets,
        })
    }

    /// Конверт с нашим входом в группу — разослать соседям.
    pub fn join_envelope(&self, _group_id: Vec<u8>) -> Result<Option<FfiOutgoing>, FfiError> {
        // Впуск теперь идёт через join_from_invite (MLS Add/Commit + Welcome).
        Ok(None)
    }

    /// Обработать входящий конверт. Возвращает конверты для пересылки (с адресатами).
    pub fn receive(&self, envelope: Vec<u8>) -> Result<Vec<FfiOutgoing>, FfiError> {
        let mut engine = self.inner.lock().unwrap();
        let outcome = engine.handle(&envelope)?;
        use prost::Message as _;
        Ok(outcome
            .outgoing
            .into_iter()
            .map(|o| FfiOutgoing {
                envelope: o.envelope.encode_to_vec(),
                targets: o.targets,
            })
            .collect())
    }

    /// Вся история группы в причинном порядке (для тестов/утилит).
    pub fn history(&self, group_id: Vec<u8>) -> Vec<FfiMessage> {
        let engine = self.inner.lock().unwrap();
        engine
            .log()
            .ordered(&group_id)
            .into_iter()
            .filter(|m| !engine.is_blocked(&m.author))
            .map(|m| to_ffi_msg(m, &engine))
            .collect()
    }

    /// Окно истории: не более `limit` сообщений до `before_id` (не включая его).
    /// `before_id = None` — последние `limit`. `has_more` — есть ли более старые.
    pub fn history_window(
        &self,
        group_id: Vec<u8>,
        before_id: Option<Vec<u8>>,
        limit: u32,
    ) -> FfiHistoryPage {
        let engine = self.inner.lock().unwrap();
        let (page, has_more) = engine
            .log()
            .page(&group_id, before_id.as_deref(), limit as usize)
            .unwrap_or_else(|_| (Vec::new(), false));
        let messages = page
            .into_iter()
            .filter(|m| !engine.is_blocked(&m.author))
            .map(|m| to_ffi_msg(m, &engine))
            .collect();
        FfiHistoryPage { messages, has_more }
    }

    /// Известные пиры.
    pub fn peers(&self) -> Vec<FfiPeer> {
        let engine = self.inner.lock().unwrap();
        engine
            .peers()
            .all()
            .map(|p| FfiPeer {
                peer_id: p.peer_id.clone(),
                last_seen: p.last_seen,
                is_relay: p.is_relay,
            })
            .collect()
    }
}

use std::path::{Path, PathBuf};

/// Путь к файлу личности рядом с БД (None для in-memory).
fn identity_path_for(db_path: &str) -> Option<PathBuf> {
    if db_path == ":memory:" {
        return None;
    }
    let dir = Path::new(db_path)
        .parent()
        .filter(|d| !d.as_os_str().is_empty());
    Some(match dir {
        Some(d) => d.join("identity.key"),
        None => PathBuf::from("identity.key"),
    })
}

/// Загрузить личность из отдельного файла; при отсутствии — разово перенести
/// из старой БД (`meta.identity`) или создать новую.
fn load_or_create_identity(
    path: Option<&Path>,
    store: &p2pchat_storage::Store,
) -> Result<p2pchat_crypto::Identity, FfiError> {
    // 1) отдельный файл
    if let Some(p) = path {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(id) = p2pchat_crypto::Identity::from_secret_bytes(&bytes) {
                return Ok(id);
            }
        }
    }
    // 2) миграция из БД (старые установки)
    if let Some(bytes) = store.get_meta("identity").map_err(io_err)? {
        let id =
            p2pchat_crypto::Identity::from_secret_bytes(&bytes).map_err(|e| FfiError::Core {
                reason: e.to_string(),
            })?;
        if let Some(p) = path {
            write_atomic(p, &id.secret_bytes()).map_err(io_err)?;
        }
        return Ok(id);
    }
    // 3) новая личность
    let id = p2pchat_crypto::Identity::generate();
    match path {
        Some(p) => write_atomic(p, &id.secret_bytes()).map_err(io_err)?,
        None => store
            .set_meta("identity", &id.secret_bytes())
            .map_err(io_err)?,
    }
    Ok(id)
}

/// Атомарная запись файла (temp + rename).
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("key.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn io_err<E: std::fmt::Display>(e: E) -> FfiError {
    FfiError::Core {
        reason: e.to_string(),
    }
}

fn to_ffi_msg(m: p2pchat_core::proto::ChatMessage, engine: &GossipEngine) -> FfiMessage {
    let deleted = engine.is_deleted(&m.id);
    let (text, edited) = if deleted {
        (String::new(), false)
    } else {
        engine.effective_text(&m.id, &m.text)
    };
    let reactions = if deleted {
        Vec::new()
    } else {
        engine
            .reactions_for(&m.id, &engine.peer_id())
            .into_iter()
            .map(|r| FfiReaction {
                emoji: r.emoji,
                count: r.count,
                mine: r.mine,
            })
            .collect()
    };
    FfiMessage {
        id: m.id,
        group_id: m.group_id,
        author: m.author,
        text,
        lamport: m.lamport,
        wall_time: m.wall_time,
        edited,
        deleted,
        attachment: if deleted {
            None
        } else {
            m.attachment.map(to_ffi_attachment)
        },
        reactions,
    }
}

fn to_ffi_attachment(a: p2pchat_core::proto::Attachment) -> FfiAttachment {
    FfiAttachment {
        kind: a.kind,
        id: a.id,
        width: a.width,
        height: a.height,
        url: a.url,
        mime: a.mime,
        size: a.size,
        duration_ms: a.duration_ms,
        file_name: a.file_name,
        thumbnail: a.thumbnail,
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[uniffi::export]
pub fn default_group_id() -> GroupId {
    b"general".to_vec()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_lives_in_separate_file_and_survives_db_reopen() {
        let dir = std::env::temp_dir().join(format!("p2pchat-ffi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("chat.db");
        let db = db.to_str().unwrap();
        let path = identity_path_for(db).unwrap();

        let id1 = {
            let store = p2pchat_storage::Store::open(db).unwrap();
            load_or_create_identity(Some(&path), &store).unwrap()
        };
        assert!(path.exists(), "файл личности создан");

        // переоткрываем БД — личность берётся из файла и совпадает
        let id2 = {
            let store = p2pchat_storage::Store::open(db).unwrap();
            load_or_create_identity(Some(&path), &store).unwrap()
        };
        assert_eq!(id1.secret_bytes(), id2.secret_bytes());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_window_paginates() {
        let core = ChatCore::new(":memory:".into()).unwrap();
        let g = b"g".to_vec();
        let _ = core.create_group_invite(g.clone());
        for i in 0..120 {
            core.post(g.clone(), format!("m{i}")).unwrap();
        }

        let page1 = core.history_window(g.clone(), None, 50);
        assert_eq!(page1.messages.len(), 50);
        assert!(page1.has_more);

        let before1 = page1.messages.first().unwrap().id.clone();
        let page2 = core.history_window(g.clone(), Some(before1), 50);
        assert_eq!(page2.messages.len(), 50);
        assert!(page2.has_more);

        let before2 = page2.messages.first().unwrap().id.clone();
        let page3 = core.history_window(g.clone(), Some(before2), 50);
        assert_eq!(page3.messages.len(), 20);
        assert!(!page3.has_more);
    }
}

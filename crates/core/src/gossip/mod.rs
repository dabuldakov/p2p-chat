//! Gossip-движок: превращает высокоуровневые действия в конверты,
//! проверяет и применяет входящие, решает что переслать дальше.
//!
//! Транспорт (WebRTC/сокеты) для этого модуля — просто «отправить байты
//! вот этому peer_id». Всё остальное — здесь.

use ed25519_dalek::VerifyingKey;
use prost::Message as _;

use crate::log::Log;
use crate::mls::{MlsGroups, Received};
use crate::peers::PeerTable;
use crate::proto::gossip::Body;
use crate::proto::{ChatMessage, Direct, Envelope, Gossip, Link, Links, Mls, PeerInfo, Presence, ProfileEvent};
use crate::{CoreError, GroupId, PeerId};

mod sync;

/// Количество соседей для пересылки (используется транспортом при выборе цели).
pub const FANOUT: usize = 5;

pub struct GossipEngine {
    identity: p2pchat_crypto::Identity,
    log: Log,
    peers: PeerTable,
    /// MLS: личность (аккаунт+устройство) и состояния групп.
    mls: MlsGroups,
    /// Путь к SQLite для снапшотов MLS (None — без персиста, напр. в тестах).
    mls_db: Option<String>,
    /// Владельцы групп (кто создал) — для выдачи билетов и удаления участников.
    owners: std::collections::HashMap<GroupId, PeerId>,
    /// Эфемерные публикации связей: group → author → (time, [(peer, kind)]).
    links: std::collections::HashMap<GroupId, std::collections::HashMap<PeerId, (u64, Vec<(Vec<u8>, String)>)>>,
    /// Никнеймы участников (peer_id → (ник, lamport)).
    nicknames: std::collections::HashMap<PeerId, (String, u64)>,
    /// nonce уже обработанных конвертов (анти-реплей, дедуп лавины)
    seen: std::collections::HashSet<u64>,
}

/// Что переслать и кому.
/// `targets` — публичные ключи получателей; пусто → разослать всем подключённым.
pub struct Outgoing {
    pub envelope: Envelope,
    pub targets: Vec<Vec<u8>>,
}

/// Что произошло при обработке конверта.
#[derive(Default)]
pub struct Outcome {
    /// Новые принятые сообщения чата.
    pub new_messages: Vec<ChatMessage>,
    /// Обновлённые пиры.
    pub new_peers: Vec<PeerInfo>,
    /// Что нужно переслать (с адресатами).
    pub outgoing: Vec<Outgoing>,
    /// Группы, по которым не хватает родителей — надо запросить синк.
    pub needs_sync: Vec<GroupId>,
}

impl GossipEngine {
    pub fn new(identity: p2pchat_crypto::Identity, log: Log) -> Self {
        let peers = PeerTable::new(identity.peer_id().to_vec());
        let mls = MlsGroups::from_crypto(&identity).expect("mls identity");
        Self {
            identity,
            log,
            peers,
            mls,
            mls_db: None,
            owners: Default::default(),
            links: Default::default(),
            nicknames: Default::default(),
            seen: Default::default(),
        }
    }

    pub fn peer_id(&self) -> PeerId {
        self.identity.peer_id().to_vec()
    }

    pub fn public_key(&self) -> Vec<u8> {
        self.identity.public_key().to_bytes().to_vec()
    }

    pub fn log(&self) -> &Log {
        &self.log
    }

    pub fn log_mut(&mut self) -> &mut Log {
        &mut self.log
    }

    pub fn peers(&self) -> &PeerTable {
        &self.peers
    }

    pub fn peers_mut(&mut self) -> &mut PeerTable {
        &mut self.peers
    }

    /// Создать сообщение и упаковать его с адресатами (соседями группы).
    pub fn post(&mut self, group: GroupId, text: String) -> Result<Outgoing, CoreError> {
        let msg = self.log.append_local(group, self.peer_id(), text, None)?;
        let out = self.out_msg(msg)?;
        self.persist_mls();
        Ok(out)
    }

    /// Создать сообщение-гифку: между клиентами едет только ссылка-идентификатор.
    pub fn post_gif(
        &mut self,
        group: GroupId,
        attachment: crate::proto::Attachment,
    ) -> Result<Outgoing, CoreError> {
        let msg = self
            .log
            .append_local(group, self.peer_id(), String::new(), Some(attachment))?;
        let out = self.out_msg(msg)?;
        self.persist_mls();
        Ok(out)
    }

    /// Создать новую группу (MLS). Ключ раздаётся попарно при впуске.
    pub fn create_group(&mut self, group: GroupId) -> Result<(), CoreError> {
        self.mls.create(&group)?;
        let _ = self.log.set_meta(&format!("owner:{}", hex(&group)), &self.peer_id());
        self.owners.insert(group.clone(), self.peer_id());
        self.remember_group(&group);
        self.persist_mls();
        Ok(())
    }

    /// Мы владелец группы (её создатель)?
    pub fn is_owner(&self, group: &[u8]) -> Result<bool, CoreError> {
        if let Some(owner) = self.owners.get(group) {
            return Ok(*owner == self.peer_id());
        }
        let key = format!("owner:{}", hex(group));
        Ok(self.log.get_meta(&key)?.as_deref() == Some(self.peer_id().as_slice()))
    }

    /// Удалить участника (только владелец): MLS Remove + Commit остальным.
    pub fn remove_member(
        &mut self,
        group: &[u8],
        peer_id: &[u8],
    ) -> Result<Vec<Outgoing>, CoreError> {
        if !self.is_owner(group)? {
            return Ok(Vec::new());
        }
        let Some(pk) = self
            .members(group)
            .into_iter()
            .find(|m| m.peer_id == peer_id)
            .map(|m| m.public_key)
        else {
            return Ok(Vec::new());
        };
        let commit = self.mls.remove_member(group, &pk)?;
        let env = self.seal(Gossip {
            body: Some(Body::Mls(Mls { group_id: group.to_vec(), message: commit })),
        })?;
        self.persist_mls();
        Ok(vec![Outgoing { envelope: env, targets: self.member_keys(group) }])
    }

    pub fn has_group(&self, group: &[u8]) -> bool {
        self.mls.has(group)
    }

    /// Все группы, в которых мы состоим.
    pub fn groups(&self) -> Vec<GroupId> {
        self.mls.group_ids()
    }

    /// Запомнить id группы для загрузки при старте.
    fn remember_group(&self, group: &[u8]) {
        let mut list: Vec<String> = self
            .log
            .get_meta("mls_groups")
            .ok()
            .flatten()
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .split('\n')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let h = hex(group);
        if !list.contains(&h) {
            list.push(h);
            let _ = self.log.set_meta("mls_groups", list.join("\n").as_bytes());
        }
    }

    /// Восстановить профили и владельцев (MLS-состояние грузится через `load_mls`).
    pub fn load_state(&mut self) -> Result<(), CoreError> {
        for (pid, nick, lamport) in self.log.load_profiles()? {
            self.nicknames.insert(pid, (nick, lamport));
        }
        Ok(())
    }

    /// Загрузить MLS-состояние (личность + группы) из SQLite.
    pub fn load_mls(&mut self, db_path: &str) -> Result<(), CoreError> {
        self.mls_db = Some(db_path.to_string());
        self.mls = MlsGroups::load(&self.identity, db_path)?;
        let ids: Vec<String> = self
            .log
            .get_meta("mls_groups")?
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .split('\n')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        for h in ids {
            if let Ok(gid) = unhex(&h) {
                self.mls.load_group(&gid)?;
                if let Some(owner) = self.log.get_meta(&format!("owner:{}", hex(&gid)))? {
                    self.owners.insert(gid.clone(), owner);
                }
            }
        }
        Ok(())
    }

    /// Создать одноразовый инвайт-билет (только владелец). `ttl_secs=0` — без срока.
    pub fn create_invite(&mut self, group: &[u8], ttl_secs: u64) -> Result<String, CoreError> {
        if !self.is_owner(group)? {
            return Err(CoreError::BadEnvelope("invite: только владелец".into()));
        }
        let now = crate::unix_now();
        let expiry = if ttl_secs == 0 { 0 } else { now + ttl_secs };
        let inv = crate::invite::Invite::ticket(group, &self.public_key(), expiry, &self.identity);
        let _ = self.log.set_meta(&format!("invite:{}", inv.use_token()), b"1");
        Ok(inv.encode())
    }

    /// Начать впуск по билету: заявка (KeyPackage + билет) хозяину, попарно.
    pub fn join_request(&self, invite: &str) -> Result<(GroupId, Outgoing), CoreError> {
        let inv = crate::invite::Invite::parse(invite)?;
        if !inv.verify_sig() {
            return Err(CoreError::BadEnvelope("invite: подпись неверна".into()));
        }
        let req = crate::proto::JoinRequest {
            group_id: inv.group_id.clone(),
            ticket: invite.to_string(),
            key_package: self.mls.key_package()?,
        };
        let pairwise = crate::proto::Pairwise {
            body: Some(crate::proto::pairwise::Body::JoinRequest(req)),
        };
        let env = self.seal_pairwise(&inv.host, &pairwise.encode_to_vec())?;
        Ok((inv.group_id, Outgoing { envelope: env, targets: vec![inv.host.clone()] }))
    }

    /// Выйти из группы: убираем её из локального состояния (список групп).
    pub fn leave_group(&mut self, group: &[u8]) -> Result<Option<Outgoing>, CoreError> {
        if !self.has_group(group) {
            return Ok(None);
        }
        self.mls.remove(group);
        self.forget_group(group);
        Ok(None)
    }

    /// Убрать id группы из списка для загрузки при старте.
    fn forget_group(&self, group: &[u8]) {
        let mut list: Vec<String> = self
            .log
            .get_meta("mls_groups")
            .ok()
            .flatten()
            .map(|b| {
                String::from_utf8_lossy(&b)
                    .split('\n')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let h = hex(group);
        if list.iter().any(|x| x == &h) {
            list.retain(|x| x != &h);
            let _ = self.log.set_meta("mls_groups", list.join("\n").as_bytes());
        }
    }

    /// Участники группы (отсортированы по peer_id).
    pub fn members(&self, group: &[u8]) -> Vec<crate::membership::Member> {
        let mut v: Vec<_> = self
            .mls
            .members(group)
            .into_iter()
            .map(|pk| crate::membership::Member {
                peer_id: p2pchat_crypto::content_id(&pk).to_vec(),
                public_key: pk,
            })
            .collect();
        v.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        v
    }

    /// Наши соседи в оверлее группы (peer_id): малая группа — full mesh,
    /// большая — кольцо с «дальними» связями (детерминированно у всех).
    pub fn neighbors(&self, group: &[u8]) -> Vec<PeerId> {
        let members: Vec<PeerId> = self.members(group).into_iter().map(|m| m.peer_id).collect();
        let me = self.peer_id();
        let Some(idx) = members.iter().position(|m| m == &me) else {
            return Vec::new();
        };
        let n = members.len();
        let others = n.saturating_sub(1);
        if others <= crate::membership::MESH_THRESHOLD {
            return members.into_iter().filter(|m| m != &me).collect();
        }
        let mut out: Vec<PeerId> = Vec::new();
        let mut offset = 1usize;
        while out.len() < FANOUT && offset < n {
            let fwd = &members[(idx + offset) % n];
            if fwd != &me && !out.contains(fwd) {
                out.push(fwd.clone());
            }
            if out.len() >= FANOUT {
                break;
            }
            let back = &members[(idx + n - offset) % n];
            if back != &me && !out.contains(back) {
                out.push(back.clone());
            }
            offset = if offset == 1 { 2 } else { offset * 2 };
        }
        out.truncate(FANOUT);
        out
    }

    /// Публичные ключи соседей (кому пересылаем / с кем соединяемся).
    pub fn neighbor_keys(&self, group: &[u8]) -> Vec<Vec<u8>> {
        let ids = self.neighbors(group);
        self.members(group)
            .into_iter()
            .filter(|m| ids.contains(&m.peer_id))
            .map(|m| m.public_key)
            .collect()
    }

    /// Публичные ключи ВСЕХ участников (кроме себя) — для handshake (Commit).
    fn member_keys(&self, group: &[u8]) -> Vec<Vec<u8>> {
        let me = self.public_key();
        self.members(group)
            .into_iter()
            .map(|m| m.public_key)
            .filter(|k| k != &me)
            .collect()
    }

    /// Сохранить MLS-состояние (если задан путь к БД).
    fn persist_mls(&self) {
        if let Some(p) = &self.mls_db {
            let _ = self.mls.save(p);
        }
    }

    /// Сообщение чата + адресаты (участники его группы).
    fn out_msg(&mut self, msg: ChatMessage) -> Result<Outgoing, CoreError> {
        let envelope = self.encrypt_env(&msg)?;
        Ok(Outgoing { envelope, targets: self.neighbor_keys(&msg.group_id) })
    }

    /// Зашифровать `ChatMessage` через MLS и упаковать в конверт.
    fn encrypt_env(&mut self, msg: &ChatMessage) -> Result<Envelope, CoreError> {
        let message = self.mls.encrypt(&msg.group_id, &msg.encode_to_vec())?;
        self.seal(Gossip {
            body: Some(Body::Mls(Mls { group_id: msg.group_id.clone(), message })),
        })
    }

    /// Попарно (X25519) зашифровать сервисный payload прямому адресату.
    fn seal_pairwise(&self, to: &[u8], payload: &[u8]) -> Result<Envelope, CoreError> {
        let vk = VerifyingKey::from_bytes(
            to.try_into()
                .map_err(|_| CoreError::BadEnvelope("bad recipient key".into()))?,
        )
        .map_err(|_| CoreError::BadEnvelope("bad recipient key".into()))?;
        let (nonce, ct) = p2pchat_crypto::pairwise_encrypt(&self.identity, &vk, payload)?;
        self.seal(Gossip {
            body: Some(Body::Direct(Direct {
                to: to.to_vec(),
                nonce: nonce.to_vec(),
                ciphertext: ct,
            })),
        })
    }

    /// Адресовать конкретному пиру (напр. ответ на синк).
    fn out_to(&self, envelope: Envelope, target: Vec<u8>) -> Outgoing {
        Outgoing { envelope, targets: vec![target] }
    }

    /// Рассылка всем подключённым.
    fn out_all(&self, envelope: Envelope) -> Outgoing {
        Outgoing { envelope, targets: Vec::new() }
    }

    /// Отправить набор сообщений конкретному пиру (ответ на синк).
    fn emit_messages_to(
        &mut self,
        msgs: Vec<ChatMessage>,
        target: Vec<u8>,
        out: &mut Outcome,
    ) -> Result<(), CoreError> {
        for m in msgs {
            let env = self.encrypt_env(&m)?;
            out.outgoing.push(self.out_to(env, target.clone()));
        }
        Ok(())
    }

    /// Обработать входящий конверт.
    pub fn handle(&mut self, bytes: &[u8]) -> Result<Outcome, CoreError> {
        let env = Envelope::decode(bytes)?;
        let origin = env.sender.clone();
        let gossip = self.open(&env)?;
        let forward_group = mls_group_of(&gossip);
        let mut out = Outcome::default();

        match gossip.body {
            Some(Body::Message(msg)) => {
                self.on_message(msg, &mut out)?;
            }
            Some(Body::PeerList(list)) => {
                self.peers.merge(&list);
                out.new_peers = list.peers;
            }
            Some(Body::SyncRequest(req)) => {
                let resp = self.make_sync_response(&req);
                self.emit_messages_to(resp.messages, origin.clone(), &mut out)?;
            }
            Some(Body::SyncResponse(resp)) => {
                for m in resp.messages {
                    self.on_message(m, &mut out)?;
                }
                self.peers.merge(&crate::proto::PeerList {
                    peers: resp.peers,
                    version: 0,
                });
            }
            Some(Body::SyncDigest(d)) => {
                self.on_digest(d, &origin, &mut out)?;
            }
            Some(Body::SyncIds(s)) => {
                self.on_sync_ids(s, &origin, &mut out)?;
            }
            Some(Body::SyncFetch(f)) => {
                let messages: Vec<ChatMessage> = f
                    .ids
                    .iter()
                    .filter_map(|id| self.log.get(id).cloned())
                    .collect();
                self.emit_messages_to(messages, origin.clone(), &mut out)?;
            }
            Some(Body::Encrypted(_)) | Some(Body::Membership(_)) => {
                // Устаревшие тела старого протокола — игнорируем.
            }
            Some(Body::Direct(d)) => {
                self.on_direct(d, &origin, &mut out)?;
            }
            Some(Body::Profile(p)) => {
                self.on_profile(p, &mut out)?;
            }
            Some(Body::Presence(p)) => {
                self.on_presence(p);
            }
            Some(Body::Mls(m)) => {
                self.on_mls(m, &mut out)?;
            }
            Some(Body::Links(l)) => {
                self.on_links(l, &origin)?;
            }
            None => {}
        }

        // MLS-трафик пересылаем как есть (шифртекст) соседям группы —
        // расшифровывать/перешифровывать на релее нельзя (forward secrecy).
        if let Some(g) = forward_group {
            let targets = self.neighbor_keys(&g);
            if !targets.is_empty() {
                out.outgoing.push(Outgoing { envelope: env.clone(), targets });
            }
        }

        // если не хватает родителей — инициируем Merkle-синк по группе
        for g in out.new_messages.iter().map(|m| m.group_id.clone()) {
            if !self.log.missing_parents(&g).is_empty() && !out.needs_sync.contains(&g) {
                out.needs_sync.push(g.clone());
                let env = self.sync_digest(&g)?;
                out.outgoing.push(self.out_all(env));
            }
        }

        self.persist_mls();
        Ok(out)
    }

    /// Группы, где мы состоим и есть участник с таким публичным ключом.
    pub fn groups_with(&self, public_key: &[u8]) -> Vec<GroupId> {
        self.groups()
            .into_iter()
            .filter(|g| self.mls.members(g).iter().any(|k| k == public_key))
            .collect()
    }

    /// Отметить группу прочитанной (до последнего сообщения).
    pub fn mark_read(&mut self, group: &[u8]) -> Result<(), CoreError> {
        if let Some(last) = self.log.ordered(group).last() {
            self.log.set_read(group, &last.id)?;
        }
        Ok(())
    }

    /// Число непрочитанных сообщений в группе.
    pub fn unread_count(&self, group: &[u8]) -> Result<u64, CoreError> {
        let ordered = self.log.ordered(group);
        let Some(marker) = self.log.read_marker(group)? else {
            return Ok(ordered.len() as u64);
        };
        match ordered.iter().position(|m| m.id == marker) {
            Some(idx) => Ok((ordered.len() - idx - 1) as u64),
            None => Ok(ordered.len() as u64),
        }
    }

    /// Разослать свою таблицу пиров.
    pub fn announce_peers(&mut self) -> Result<Envelope, CoreError> {
        let list = self.peers.snapshot();
        self.seal(Gossip { body: Some(Body::PeerList(list)) })
    }

fn on_message(&mut self, msg: ChatMessage, out: &mut Outcome) -> Result<(), CoreError> {
        // отмечаем автора как живым
        self.touch_peer(&msg.author);
        if self.log.merge(msg.clone())? {
            out.new_messages.push(msg);
        }
        Ok(())
    }

    /// Обработать MLS-сообщение: прикладное → в лог; handshake — уже применён.
    ///
    /// Ошибки (дубликат при лавине, свой же Commit, устаревший epoch) не фатальны —
    /// игнорируем: доставка идемпотентна, состояние уже применено.
    fn on_mls(&mut self, m: Mls, out: &mut Outcome) -> Result<(), CoreError> {
        if !self.mls.has(&m.group_id) {
            return Ok(()); // неизвестная группа (например, Commit до Welcome)
        }
        if let Ok(Received::Application(bytes)) = self.mls.process(&m.group_id, &m.message) {
            let msg = ChatMessage::decode(bytes.as_slice())?;
            self.on_message(msg, out)?;
        }
        Ok(())
    }

    fn on_presence(&mut self, p: Presence) {
        if p.peer_id == self.peer_id() {
            return;
        }
        self.peers.upsert(PeerInfo {
            peer_id: p.peer_id,
            endpoints: Vec::new(),
            last_seen: p.since,
            is_relay: false,
            hops: 1,
        });
    }

    fn touch_peer(&mut self, peer: &[u8]) {
        if peer == self.peer_id() {
            return;
        }
        self.peers.upsert(PeerInfo {
            peer_id: peer.to_vec(),
            endpoints: Vec::new(),
            last_seen: crate::unix_now(),
            is_relay: false,
            hops: 1,
        });
    }

    /// Попарно зашифрованный сервисный пакет: впуск (заявка/Welcome).
    fn on_direct(&mut self, d: Direct, origin: &[u8], out: &mut Outcome) -> Result<(), CoreError> {
        if d.to != self.public_key() {
            return Ok(());
        }
        let Ok(sender_vk) = VerifyingKey::from_bytes(origin.try_into().unwrap_or(&[0u8; 32])) else {
            return Ok(());
        };
        let Ok(nonce) = <[u8; 12]>::try_from(d.nonce.as_slice()) else {
            return Ok(());
        };
        let Ok(plain) =
            p2pchat_crypto::pairwise_decrypt(&self.identity, &sender_vk, &nonce, &d.ciphertext)
        else {
            return Ok(());
        };
        let Ok(pw) = crate::proto::Pairwise::decode(plain.as_slice()) else {
            return Ok(());
        };
        if let Some(body) = pw.body {
            match body {
                crate::proto::pairwise::Body::JoinRequest(req) => {
                    self.on_join_request(req, origin, out)?;
                }
                crate::proto::pairwise::Body::JoinWelcome(w) => {
                    let gid = self.mls.join(&w.welcome)?;
                    self.remember_group(&gid);
                    self.persist_mls();
                }
            }
        }
        Ok(())
    }

    /// Заявка на вход: проверяем билет, добавляем участника, шлём Commit + Welcome.
    fn on_join_request(
        &mut self,
        req: crate::proto::JoinRequest,
        origin: &[u8],
        out: &mut Outcome,
    ) -> Result<(), CoreError> {
        let Ok(inv) = crate::invite::Invite::parse(&req.ticket) else {
            return Ok(());
        };
        if inv.group_id != req.group_id
            || inv.host != self.public_key()
            || !inv.verify_sig()
            || inv.is_expired(crate::unix_now())
        {
            return Ok(());
        }
        let token = inv.use_token();
        if self.log.get_meta(&format!("invite:{token}"))?.is_none() {
            return Ok(()); // не наш билет
        }
        if self.log.get_meta(&format!("used:{token}"))?.is_some() {
            return Ok(()); // одноразовый: уже использован
        }
        if !self.mls.has(&req.group_id) {
            return Ok(());
        }
        let (commit, welcome) = self.mls.add_member(&req.group_id, &req.key_package)?;
        let _ = self.log.set_meta(&format!("used:{token}"), b"1");
        let commit_env = self.seal(Gossip {
            body: Some(Body::Mls(Mls { group_id: req.group_id.clone(), message: commit })),
        })?;
        out.outgoing
            .push(Outgoing { envelope: commit_env, targets: self.member_keys(&req.group_id) });
        let pw = crate::proto::Pairwise {
            body: Some(crate::proto::pairwise::Body::JoinWelcome(crate::proto::JoinWelcome {
                group_id: req.group_id.clone(),
                welcome,
            })),
        };
        let wel_env = self.seal_pairwise(origin, &pw.encode_to_vec())?;
        out.outgoing.push(Outgoing { envelope: wel_env, targets: vec![origin.to_vec()] });
        Ok(())
    }

    /// Принять публикацию связей (эфемерно, LWW по времени автора).
    fn on_links(&mut self, l: Links, origin: &[u8]) -> Result<(), CoreError> {
        // принимаем только от участника группы и с корректным peer_id
        let is_member = self.members(&l.group_id).iter().any(|m| m.public_key == origin);
        if !is_member || p2pchat_crypto::content_id(origin).to_vec() != l.peer_id {
            return Ok(());
        }
        let snap: Vec<(Vec<u8>, String)> =
            l.links.iter().map(|k| (k.peer_id.clone(), k.kind.clone())).collect();
        let e = self.links.entry(l.group_id).or_default();
        let cur = e.get(&l.peer_id).map(|(t, _)| *t).unwrap_or(0);
        if l.time >= cur {
            e.insert(l.peer_id, (l.time, snap));
        }
        Ok(())
    }

    /// Опубликовать свои активные связи в группе (рассылается соседям).
    pub fn post_links(
        &mut self,
        group: &[u8],
        links: Vec<(Vec<u8>, String)>,
    ) -> Result<Option<Outgoing>, CoreError> {
        if !self.mls.has(group) {
            return Ok(None);
        }
        let me = self.peer_id();
        let msg = Links {
            group_id: group.to_vec(),
            peer_id: me.clone(),
            time: crate::unix_now(),
            links: links
                .into_iter()
                .map(|(p, k)| Link { peer_id: p, kind: k })
                .collect(),
        };
        let snap: Vec<(Vec<u8>, String)> =
            msg.links.iter().map(|k| (k.peer_id.clone(), k.kind.clone())).collect();
        self.links
            .entry(group.to_vec())
            .or_default()
            .insert(me, (msg.time, snap));
        let env = self.seal(Gossip { body: Some(Body::Links(msg)) })?;
        Ok(Some(Outgoing { envelope: env, targets: self.neighbor_keys(group) }))
    }

    /// Снимок связей группы: список (author, peer, kind).
    pub fn links(&self, group: &[u8]) -> Vec<(Vec<u8>, Vec<u8>, String)> {
        let mut out = Vec::new();
        if let Some(authors) = self.links.get(group) {
            for (author, (_t, ls)) in authors {
                for (peer, kind) in ls {
                    out.push((author.clone(), peer.clone(), kind.clone()));
                }
            }
        }
        out
    }

    // --- Профиль (никнейм) ----------------------------------------------

    /// Задать свой никнейм и вернуть конверт для рассылки.
    pub fn set_nickname(&mut self, name: &str) -> Result<Option<Outgoing>, CoreError> {
        let pid = self.peer_id();
        let lamport = self.log.tick();
        self.nicknames.insert(pid.clone(), (name.to_string(), lamport));
        let _ = self.log.store_profile(&pid, name, lamport);
        Ok(Some(self.profile_outgoing()?))
    }

    pub fn my_nickname(&self) -> Option<String> {
        self.nicknames.get(&self.peer_id()).map(|(n, _)| n.clone())
    }

    pub fn nickname_of(&self, peer_id: &[u8]) -> Option<String> {
        self.nicknames.get(peer_id).map(|(n, _)| n.clone())
    }

    pub fn all_nicknames(&self) -> Vec<(PeerId, String)> {
        self.nicknames.iter().map(|(k, (n, _))| (k.clone(), n.clone())).collect()
    }

    /// Конверт с нашим профилем (для повторной рассылки), если ник задан.
    pub fn profile_envelope(&self) -> Result<Option<Outgoing>, CoreError> {
        if self.nicknames.contains_key(&self.peer_id()) {
            Ok(Some(self.profile_outgoing()?))
        } else {
            Ok(None)
        }
    }

    fn profile_outgoing(&self) -> Result<Outgoing, CoreError> {
        let pid = self.peer_id();
        let (name, lamport) = self
            .nicknames
            .get(&pid)
            .cloned()
            .unwrap_or((String::new(), 0));
        let ev = crate::membership::profile_event(&pid, &name, lamport, &pid);
        let env = self.seal(Gossip { body: Some(Body::Profile(ev)) })?;
        Ok(self.out_all(env))
    }

    fn on_profile(&mut self, ev: ProfileEvent, out: &mut Outcome) -> Result<(), CoreError> {
        let wins = match self.nicknames.get(&ev.peer_id) {
            None => true,
            Some((_, l)) => ev.lamport > *l,
        };
        if wins {
            self.nicknames
                .insert(ev.peer_id.clone(), (ev.nickname.clone(), ev.lamport));
            let _ = self.log.store_profile(&ev.peer_id, &ev.nickname, ev.lamport);
            let env = self.seal(Gossip { body: Some(Body::Profile(ev)) })?;
            out.outgoing.push(self.out_all(env));
        }
        Ok(())
    }

    /// Подписать и упаковать gossip в конверт.
    fn seal(&self, gossip: Gossip) -> Result<Envelope, CoreError> {
        let payload = gossip.encode_to_vec();
        let signature = self.identity.sign(&payload);
        Ok(Envelope {
            sender: self.identity.public_key().to_bytes().to_vec(),
            nonce: rand::random(),
            payload,
            signature,
            lamport: self.log.lamport(),
        })
    }

    /// Проверить подпись и распаковать.
    fn open(&mut self, env: &Envelope) -> Result<Gossip, CoreError> {
        if !self.seen.insert(env.nonce) {
            return Err(CoreError::BadEnvelope("replay".into()));
        }
        let pk_bytes: [u8; 32] = env
            .sender
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::BadEnvelope("bad sender key".into()))?;
        let pk = VerifyingKey::from_bytes(&pk_bytes)
            .map_err(|_| CoreError::BadEnvelope("bad key".into()))?;
        p2pchat_crypto::verify(&pk, &env.payload, &env.signature)?;
        Ok(Gossip::decode(env.payload.as_slice())?)
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
    if !s.len().is_multiple_of(2) {
        return Err(CoreError::BadEnvelope("odd hex".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| CoreError::BadEnvelope("bad hex".into()))
        })
        .collect()
}

/// id группы MLS-сообщения (для пересылки).
fn mls_group_of(g: &Gossip) -> Option<GroupId> {
    match &g.body {
        Some(Body::Mls(m)) => Some(m.group_id.clone()),
        Some(Body::Links(l)) => Some(l.group_id.clone()),
        _ => None,
    }
}

/// Разобрать инвайт (совместимость). Подробнее — [`crate::invite`].
pub fn parse_invite(invite: &str) -> Result<(GroupId, Vec<u8>), CoreError> {
    let inv = crate::invite::Invite::parse(invite)?;
    Ok((inv.group_id, inv.host))
}
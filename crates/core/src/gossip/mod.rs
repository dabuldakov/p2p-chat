//! Gossip-движок: превращает высокоуровневые действия в конверты,
//! проверяет и применяет входящие, решает что переслать дальше.
//!
//! Транспорт (WebRTC/сокеты) для этого модуля — просто «отправить байты
//! вот этому peer_id». Всё остальное — здесь.

use ed25519_dalek::VerifyingKey;
use prost::Message as _;

use crate::log::Log;
use crate::peers::PeerTable;
use crate::proto::gossip::Body;
use crate::proto::{ChatMessage, Direct, Encrypted, Envelope, Gossip, PeerInfo, Presence, ProfileEvent};
use crate::{CoreError, GroupId, PeerId};

use p2pchat_crypto::GroupKey;

mod sync;

/// Количество соседей для пересылки (используется транспортом при выборе цели).
pub const FANOUT: usize = 5;

pub struct GossipEngine {
    identity: p2pchat_crypto::Identity,
    log: Log,
    peers: PeerTable,
    /// Групповые ключи: группа -> симметричный ключ шифрования payload.
    groups: std::collections::HashMap<GroupId, GroupKey>,
    /// Состав каждой группы (CRDT). Из него считается топология оверлея.
    membership: std::collections::HashMap<GroupId, crate::membership::Membership>,
    /// Наше собственное событие входа в группу (для рассылки).
    self_events: std::collections::HashMap<GroupId, crate::proto::MembershipEvent>,
    /// Владельцы групп (кто создал) — для удаления участников.
    owners: std::collections::HashMap<GroupId, PeerId>,
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
        Self { identity, log, peers, groups: Default::default(), membership: Default::default(), self_events: Default::default(), owners: Default::default(), nicknames: Default::default(), seen: Default::default() }
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
        self.out_msg(msg)
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
        self.out_msg(msg)
    }

    /// Создать новую группу: генерирует ключ, помечает себя участником.
    /// Возвращает ключ группы (его раздаём по инвайту).
    pub fn create_group(&mut self, group: GroupId) -> [u8; 32] {
        let key = GroupKey::generate();
        let bytes = key.export();
        self.groups.insert(group.clone(), key);
        let _ = self.log.store_group_key(&group, &bytes);
        // владелец группы (создатель) — для удаления участников
        let _ = self.log.set_meta(&format!("owner:{}", hex(&group)), &self.peer_id());
        self.owners.insert(group.clone(), self.peer_id());
        self.self_join(&group);
        bytes
    }

    /// Мы владелец группы (её создатель)?
    pub fn is_owner(&self, group: &[u8]) -> Result<bool, CoreError> {
        if let Some(owner) = self.owners.get(group) {
            return Ok(*owner == self.peer_id());
        }
        let key = format!("owner:{}", hex(group));
        Ok(self.log.get_meta(&key)?.as_deref() == Some(self.peer_id().as_slice()))
    }

    /// Удалить участника (только владелец): создаём событие выхода за него
    /// и, если мы лидер, сразу меняем ключ группы.
    pub fn remove_member(
        &mut self,
        group: &[u8],
        peer_id: &[u8],
    ) -> Result<Vec<Outgoing>, CoreError> {
        if !self.is_owner(group)? {
            return Ok(Vec::new());
        }
        let Some(pk) = self.membership.get(group).and_then(|m| m.public_key_of(peer_id)) else {
            return Ok(Vec::new());
        };
        let lamport = self.log.tick();
        let ev = crate::membership::membership_event(
            group,
            peer_id,
            &pk,
            false,
            lamport,
            &self.peer_id(),
        );
        self.membership
            .entry(group.to_vec())
            .or_default()
            .apply(&ev);
        let _ = self.log.store_membership(&ev);
        let env = self.seal(Gossip {
            body: Some(Body::Membership(ev)),
        })?;
        let mut out = vec![Outgoing { envelope: env, targets: self.neighbor_keys(group) }];
        if self.is_leader(group) {
            out.extend(self.rekey(group)?);
        }
        Ok(out)
    }

    /// Вступить в группу, зная её ключ.
    pub fn join_group(&mut self, group: GroupId, key: [u8; 32]) {
        self.groups.insert(group.clone(), GroupKey::from_bytes(key));
        let _ = self.log.store_group_key(&group, &key);
        self.self_join(&group);
    }

    pub fn has_group_key(&self, group: &[u8]) -> bool {
        self.groups.contains_key(group)
    }

    /// Все группы, в которых мы состоим (по членству).
    pub fn groups(&self) -> Vec<GroupId> {
        self.membership
            .iter()
            .filter(|(_, m)| m.is_member(&self.peer_id()))
            .map(|(g, _)| g.clone())
            .collect()
    }

    /// Состоим ли мы в группе.
    pub fn has_group(&self, group: &[u8]) -> bool {
        self.membership
            .get(group)
            .map(|m| m.is_member(&self.peer_id()))
            .unwrap_or(false)
    }

    /// Пометить себя участником (локально) и запомнить событие для рассылки.
    fn self_join(&mut self, group: &[u8]) {
        let lamport = self.log.tick();
        let ev = crate::membership::membership_event(
            group,
            &self.peer_id(),
            &self.public_key(),
            true,
            lamport,
            &self.peer_id(),
        );
        self.membership
            .entry(group.to_vec())
            .or_default()
            .apply(&ev.clone());
        self.self_events.insert(group.to_vec(), ev.clone());
        let _ = self.log.store_membership(&ev);
    }

    /// Восстановить группы и состав из хранилища (при старте).
    pub fn load_state(&mut self) -> Result<(), CoreError> {
        for (g, k) in self.log.load_group_keys()? {
            if let Ok(kb) = <[u8; 32]>::try_from(k.as_slice()) {
                self.groups.insert(g, GroupKey::from_bytes(kb));
            }
        }
        for ev in self.log.load_membership()? {
            self.membership
                .entry(ev.group_id.clone())
                .or_default()
                .apply(&ev);
        }
        for (pid, nick, lamport) in self.log.load_profiles()? {
            self.nicknames.insert(pid, (nick, lamport));
        }
        // заново объявить себя во всех группах, где мы состоим
        for g in self.groups() {
            if let Some(owner) = self.log.get_meta(&format!("owner:{}", hex(&g)))? {
                self.owners.insert(g.clone(), owner);
            }
            self.self_join(&g);
        }
        Ok(())
    }

    /// Конверт с нашим входом в группу — чтобы разослать соседям.
    pub fn join_envelope(&self, group: &[u8]) -> Result<Option<Outgoing>, CoreError> {
        match self.self_events.get(group) {
            Some(ev) => {
                let env = self.seal(Gossip {
                    body: Some(Body::Membership(ev.clone())),
                })?;
                Ok(Some(Outgoing { envelope: env, targets: self.neighbor_keys(group) }))
            }
            None => Ok(None),
        }
    }

    /// Инвайт-строка группы (ссылка/QR). См. [`crate::invite`].
    pub fn invite_for(&self, group: &[u8]) -> Option<String> {
        let key = self.groups.get(group)?.export();
        Some(crate::invite::Invite::new(group, &key, &self.public_key()).encode())
    }

    /// Вступить по инвайт-строке (возвращает id группы).
    pub fn join_from_invite(&mut self, invite: &str) -> Result<GroupId, CoreError> {
        let inv = crate::invite::Invite::parse(invite)?;
        self.join_group(inv.group_id.clone(), inv.key);
        Ok(inv.group_id)
    }

    /// Вступить по инвайту, вернуть группу и ключ хозяина (для bootstrap-соединения).
    pub fn join_from_invite_full(&mut self, invite: &str) -> Result<(GroupId, Vec<u8>), CoreError> {
        let inv = crate::invite::Invite::parse(invite)?;
        self.join_group(inv.group_id.clone(), inv.key);
        Ok((inv.group_id, inv.host))
    }

    /// Выйти из группы: помечаем себя вышедшим и сообщаем соседям.
    pub fn leave_group(&mut self, group: &[u8]) -> Result<Option<Outgoing>, CoreError> {
        if !self.has_group(group) {
            return Ok(None);
        }
        let lamport = self.log.tick();
        let ev = crate::membership::membership_event(
            group,
            &self.peer_id(),
            &self.public_key(),
            false,
            lamport,
            &self.peer_id(),
        );
        self.membership
            .entry(group.to_vec())
            .or_default()
            .apply(&ev);
        let _ = self.log.store_membership(&ev);
        self.self_events.remove(group);
        let env = self.seal(Gossip {
            body: Some(Body::Membership(ev)),
        })?;
        Ok(Some(Outgoing { envelope: env, targets: self.neighbor_keys(group) }))
    }

    /// Участники группы (отсортированы).
    pub fn members(&self, group: &[u8]) -> Vec<crate::membership::Member> {
        self.membership.get(group).map(|m| m.members()).unwrap_or_default()
    }

    /// Наши соседи в оверлее группы (peer_id).
    pub fn neighbors(&self, group: &[u8]) -> Vec<PeerId> {
        self.membership
            .get(group)
            .map(|m| m.neighbors(&self.peer_id(), crate::membership::DEFAULT_FANOUT))
            .unwrap_or_default()
    }

    /// Публичные ключи соседей (кому пересылаем / с кем соединяемся).
    pub fn neighbor_keys(&self, group: &[u8]) -> Vec<Vec<u8>> {
        let Some(m) = self.membership.get(group) else {
            return Vec::new();
        };
        m.neighbors(&self.peer_id(), crate::membership::DEFAULT_FANOUT)
            .into_iter()
            .filter_map(|p| m.public_key_of(&p))
            .collect()
    }

    /// Сообщение чата + адресаты (соседи его группы).
    fn out_msg(&self, msg: ChatMessage) -> Result<Outgoing, CoreError> {
        let envelope = self.seal_body_message(msg.clone())?;
        Ok(Outgoing { envelope, targets: self.neighbor_keys(&msg.group_id) })
    }

    /// Адресовать конкретному пиру (напр. ответ на синк).
    fn out_to(&self, envelope: Envelope, target: Vec<u8>) -> Outgoing {
        Outgoing { envelope, targets: vec![target] }
    }

    /// Рассылка всем подключённым.
    fn out_all(&self, envelope: Envelope) -> Outgoing {
        Outgoing { envelope, targets: Vec::new() }
    }

    /// Упаковать сообщение чата: зашифровать, если есть ключ группы.
    fn seal_body_message(&self, msg: ChatMessage) -> Result<Envelope, CoreError> {
        let body = match self.groups.get(&msg.group_id) {
            Some(key) => {
                let (nonce, ct) = key.encrypt(&msg.encode_to_vec())?;
                Body::Encrypted(Encrypted {
                    group_id: msg.group_id.clone(),
                    nonce: nonce.to_vec(),
                    ciphertext: ct,
                })
            }
            None => Body::Message(msg),
        };
        self.seal(Gossip { body: Some(body) })
    }

    /// Отправить набор сообщений конкретному пиру (ответ на синк).
    fn emit_messages_to(
        &self,
        msgs: Vec<ChatMessage>,
        target: Vec<u8>,
        out: &mut Outcome,
    ) -> Result<(), CoreError> {
        for m in msgs {
            let env = self.seal_body_message(m)?;
            out.outgoing.push(self.out_to(env, target.clone()));
        }
        Ok(())
    }

    /// Обработать входящий конверт.
    pub fn handle(&mut self, bytes: &[u8]) -> Result<Outcome, CoreError> {
        let env = Envelope::decode(bytes)?;
        let origin = env.sender.clone();
        let gossip = self.open(&env)?;
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
            Some(Body::Encrypted(enc)) => {
                self.on_encrypted(enc, &mut out)?;
            }
            Some(Body::Membership(ev)) => {
                self.on_membership(ev, &origin, &mut out)?;
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
            None => {}
        }

        // после применения — распространяем новое дальше
        let forward: Vec<ChatMessage> = out.new_messages.clone();
        for m in forward {
            out.outgoing.push(self.out_msg(m)?);
        }

        // если не хватает родителей — инициируем Merkle-синк по группе
        for g in out.new_messages.iter().map(|m| m.group_id.clone()) {
            if !self.log.missing_parents(&g).is_empty() && !out.needs_sync.contains(&g) {
                out.needs_sync.push(g.clone());
                let env = self.sync_digest(&g)?;
                out.outgoing.push(self.out_all(env));
            }
        }

        Ok(out)
    }

    /// Группы, где мы состоим и есть участник с таким публичным ключом.
    pub fn groups_with(&self, public_key: &[u8]) -> Vec<GroupId> {
        self.membership
            .iter()
            .filter(|(_, m)| {
                m.is_member(&self.peer_id())
                    && m.members().iter().any(|x| x.public_key == public_key)
            })
            .map(|(g, _)| g.clone())
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

    /// Расшифровать групповое сообщение. Без ключа группы — молча пропускаем
    /// (конверт всё равно прошёл проверку подписи и может быть переслан дальше).
    fn on_encrypted(&mut self, enc: Encrypted, out: &mut Outcome) -> Result<(), CoreError> {
        let Some(key_bytes) = self.groups.get(&enc.group_id).map(|k| k.export()) else {
            return Ok(());
        };
        let key = GroupKey::from_bytes(key_bytes);
        let nonce: [u8; 12] = enc
            .nonce
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::BadEnvelope("bad nonce".into()))?;
        // Не смогли расшифровать (чужой/устаревший ключ) — не фатально:
        // целостность уже проверена подписью, конверт можно переслать дальше.
        let Ok(pt) = key.decrypt(&nonce, &enc.ciphertext) else {
            return Ok(());
        };
        let msg = ChatMessage::decode(pt.as_slice())?;
        self.on_message(msg, out)?;
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

    /// Обработать событие членства: обновить состав и переслать дальше.
    fn on_membership(
        &mut self,
        ev: crate::proto::MembershipEvent,
        origin: &[u8],
        out: &mut Outcome,
    ) -> Result<(), CoreError> {
        let group = ev.group_id.clone();
        let from_other = ev.peer_id != self.peer_id();
        let is_leave = !ev.join;
        let is_join = ev.join;
        let changed = self
            .membership
            .entry(group.clone())
            .or_default()
            .apply(&ev);
        if changed {
            let _ = self.log.store_membership(&ev);
            let env = self.seal(Gossip {
                body: Some(Body::Membership(ev)),
            })?;
            out.outgoing.push(Outgoing { envelope: env, targets: self.neighbor_keys(&group) });
        }
        // Взаимное знакомство: если узнали о НОВОМ участнике — представимся ему.
        if changed
            && is_join
            && from_other
            && self.membership.get(&group).map(|m| m.is_member(&self.peer_id())).unwrap_or(false)
        {
            if let Some(self_ev) = self.self_events.get(&group).cloned() {
                let env = self.seal(Gossip { body: Some(Body::Membership(self_ev)) })?;
                out.outgoing.push(self.out_to(env, origin.to_vec()));
            }
        }
        // Выход участника: если мы лидер среди оставшихся — меняем ключ группы,
        // чтобы вышедший не читал новые сообщения (rekey).
        if changed && is_leave && from_other && self.is_leader(&group) {
            for o in self.rekey(&group)? {
                out.outgoing.push(o);
            }
        }
        Ok(())
    }

    /// Мы лидер группы (минимальный peer_id среди участников)?
    fn is_leader(&self, group: &[u8]) -> bool {
        let members = self.members(group);
        match members.iter().map(|m| m.peer_id.clone()).min() {
            Some(min_id) => min_id == self.peer_id(),
            None => false,
        }
    }

    /// Сменить ключ группы и раздать новый оставшимся (попарно зашифрованно).
    pub fn rekey(&mut self, group: &[u8]) -> Result<Vec<Outgoing>, CoreError> {
        let key = GroupKey::generate();
        let key_bytes = key.export();
        self.groups.insert(group.to_vec(), key);
        let _ = self.log.store_group_key(group, &key_bytes);

        let mut payload = group.to_vec();
        payload.extend_from_slice(&key_bytes);

        let mut out = Vec::new();
        for m in self.members(group) {
            if m.peer_id == self.peer_id() {
                continue;
            }
            let Ok(vk) = VerifyingKey::from_bytes(
                m.public_key.as_slice().try_into().unwrap_or(&[0u8; 32]),
            ) else {
                continue;
            };
            let (nonce, ct) = p2pchat_crypto::pairwise_encrypt(&self.identity, &vk, &payload)?;
            let env = self.seal(Gossip {
                body: Some(Body::Direct(Direct {
                    to: m.public_key.clone(),
                    nonce: nonce.to_vec(),
                    ciphertext: ct,
                })),
            })?;
            out.push(Outgoing { envelope: env, targets: vec![m.public_key] });
        }
        Ok(out)
    }

    /// Получили попарно зашифрованный прямой пакет (обычно — новый ключ группы).
    fn on_direct(&mut self, d: Direct, origin: &[u8], _out: &mut Outcome) -> Result<(), CoreError> {
        if d.to != self.public_key() {
            return Ok(()); // не нам
        }
        let Ok(sender_vk) = VerifyingKey::from_bytes(
            origin.try_into().unwrap_or(&[0u8; 32]),
        ) else {
            return Ok(());
        };
        let nonce: [u8; 12] = match d.nonce.as_slice().try_into() {
            Ok(n) => n,
            Err(_) => return Ok(()),
        };
        let Ok(plain) = p2pchat_crypto::pairwise_decrypt(&self.identity, &sender_vk, &nonce, &d.ciphertext)
        else {
            return Ok(());
        };
        if plain.len() <= 32 {
            return Ok(());
        }
        let (group, key_bytes) = plain.split_at(plain.len() - 32);
        let Ok(key) = <[u8; 32]>::try_from(key_bytes) else {
            return Ok(());
        };
        // отправитель должен быть участником группы
        let sender_id = p2pchat_crypto::peer_id_of(&sender_vk);
        let is_member = self
            .membership
            .get(group)
            .map(|m| m.is_member(&sender_id))
            .unwrap_or(false);
        if !is_member {
            return Ok(());
        }
        self.groups.insert(group.to_vec(), GroupKey::from_bytes(key));
        let _ = self.log.store_group_key(group, &key);
        Ok(())
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

/// Разобрать инвайт (совместимость). Подробнее — [`crate::invite`].
pub fn parse_invite(invite: &str) -> Result<(GroupId, [u8; 32]), CoreError> {
    let inv = crate::invite::Invite::parse(invite)?;
    Ok((inv.group_id, inv.key))
}
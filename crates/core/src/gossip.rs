//! Gossip-движок: превращает высокоуровневые действия в конверты,
//! проверяет и применяет входящие, решает что переслать дальше.
//!
//! Транспорт (WebRTC/сокеты) для этого модуля — просто «отправить байты
//! вот этому peer_id». Всё остальное — здесь.

use ed25519_dalek::VerifyingKey;
use prost::Message as _;

use crate::log::Log;
use crate::merkle::MerkleTree;
use crate::peers::PeerTable;
use crate::proto::gossip::Body;
use crate::proto::{
    ChatMessage, Encrypted, Envelope, Gossip, PeerInfo, Presence, SyncDigest, SyncFetch, SyncIds,
    SyncRequest, SyncResponse,
};
use crate::{CoreError, GroupId, PeerId};

use p2pchat_crypto::GroupKey;

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
        Self { identity, log, peers, groups: Default::default(), membership: Default::default(), self_events: Default::default(), seen: Default::default() }
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
        let msg = self.log.append_local(group, self.peer_id(), text)?;
        self.out_msg(msg)
    }

    /// Создать новую группу: генерирует ключ, помечает себя участником.
    /// Возвращает ключ группы (его раздаём по инвайту).
    pub fn create_group(&mut self, group: GroupId) -> [u8; 32] {
        let key = GroupKey::generate();
        let bytes = key.export();
        self.groups.insert(group.clone(), key);
        let _ = self.log.store_group_key(&group, &bytes);
        self.self_join(&group);
        bytes
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
        // заново объявить себя во всех группах, где мы состоим
        for g in self.groups() {
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

    /// Инвайт-строка группы (ссылка/QR): `p2pchat://join?g=<hex>&k=<hex>`.
    pub fn invite_for(&self, group: &[u8]) -> Option<String> {
        let key = self.groups.get(group)?.export();
        Some(format!("p2pchat://join?g={}&k={}", hex(group), hex(&key)))
    }

    /// Вступить по инвайт-строке.
    pub fn join_from_invite(&mut self, invite: &str) -> Result<GroupId, CoreError> {
        let (group, key) = parse_invite(invite)?;
        self.join_group(group.clone(), key);
        Ok(group)
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
                self.on_membership(ev, &mut out)?;
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

    /// Merkle-digest группы: корень + количество.
    pub fn sync_digest(&self, group: &[u8]) -> Result<Envelope, CoreError> {
        let tree = MerkleTree::build(self.log.ids_for_group(group));
        let d = SyncDigest {
            group_id: group.to_vec(),
            root: tree.root().to_vec(),
            count: tree.len() as u64,
        };
        self.seal(Gossip { body: Some(Body::SyncDigest(d)) })
    }

    fn on_digest(&mut self, d: SyncDigest, origin: &[u8], out: &mut Outcome) -> Result<(), CoreError> {
        let local = MerkleTree::build(self.log.ids_for_group(&d.group_id));
        let remote_root: [u8; 32] = d.root.as_slice().try_into().unwrap_or([0u8; 32]);
        if local.root() == remote_root && local.len() as u64 == d.count {
            return Ok(());
        }
        // расходимся — отвечаем отправителю своим списком id
        let ids = self.log.ids_for_group(&d.group_id);
        let env = self.seal(Gossip {
            body: Some(Body::SyncIds(SyncIds { group_id: d.group_id, ids })),
        })?;
        out.outgoing.push(self.out_to(env, origin.to_vec()));
        Ok(())
    }

    fn on_sync_ids(&mut self, s: SyncIds, origin: &[u8], out: &mut Outcome) -> Result<(), CoreError> {
        let local = MerkleTree::build(self.log.ids_for_group(&s.group_id));
        let remote = MerkleTree::build(s.ids);

        // отдаём то, чего нет у собеседника (ему)
        let to_send = local.missing_from(&remote);
        if !to_send.is_empty() {
            let messages: Vec<ChatMessage> = to_send
                .iter()
                .filter_map(|id| self.log.get(id).cloned())
                .collect();
            self.emit_messages_to(messages, origin.to_vec(), out)?;
        }

        // просим то, чего нет у нас (у него)
        let to_fetch = remote.missing_from(&local);
        if !to_fetch.is_empty() {
            let env = self.seal(Gossip {
                body: Some(Body::SyncFetch(SyncFetch {
                    group_id: s.group_id,
                    ids: to_fetch,
                })),
            })?;
            out.outgoing.push(self.out_to(env, origin.to_vec()));
        }
        Ok(())
    }

    /// Сформировать запрос синка по группе.
    pub fn sync_request(&self, group: &[u8]) -> Result<Envelope, CoreError> {
        let req = SyncRequest {
            group_id: group.to_vec(),
            have_heads: self.log.heads(group),
            lamport_watermark: self.log.lamport(),
        };
        self.seal(Gossip { body: Some(Body::SyncRequest(req)) })
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
        out: &mut Outcome,
    ) -> Result<(), CoreError> {
        let changed = self
            .membership
            .entry(ev.group_id.clone())
            .or_default()
            .apply(&ev);
        if changed {
            let _ = self.log.store_membership(&ev);
            let group = ev.group_id.clone();
            let env = self.seal(Gossip {
                body: Some(Body::Membership(ev)),
            })?;
            out.outgoing.push(Outgoing { envelope: env, targets: self.neighbor_keys(&group) });
        }
        Ok(())
    }

    fn make_sync_response(&self, req: &SyncRequest) -> SyncResponse {
        let messages: Vec<ChatMessage> = self
            .log
            .ordered(&req.group_id)
            .into_iter()
            .filter(|m| m.lamport > req.lamport_watermark)
            .take(256)
            .collect();
        SyncResponse {
            messages,
            peers: self.peers.all().cloned().collect(),
            more: false,
        }
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

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, CoreError> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err(CoreError::BadEnvelope("hex length".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| CoreError::BadEnvelope("hex".into())))
        .collect()
}

/// Разобрать инвайт `p2pchat://join?g=<hex>&k=<hex>`.
pub fn parse_invite(invite: &str) -> Result<(GroupId, [u8; 32]), CoreError> {
    let query = invite
        .split('?')
        .nth(1)
        .ok_or_else(|| CoreError::BadEnvelope("invite: no query".into()))?;
    let (mut g, mut k) = (None, None);
    for part in query.split('&') {
        let mut it = part.splitn(2, '=');
        match (it.next(), it.next()) {
            (Some("g"), Some(v)) => g = Some(v.to_string()),
            (Some("k"), Some(v)) => k = Some(v.to_string()),
            _ => {}
        }
    }
    let group = unhex(&g.ok_or_else(|| CoreError::BadEnvelope("invite: no group".into()))?)?;
    let keyv = unhex(&k.ok_or_else(|| CoreError::BadEnvelope("invite: no key".into()))?)?;
    let key: [u8; 32] = keyv
        .as_slice()
        .try_into()
        .map_err(|_| CoreError::BadEnvelope("invite: bad key".into()))?;
    Ok((group, key))
}
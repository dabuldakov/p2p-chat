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
    /// nonce уже обработанных конвертов (анти-реплей, дедуп лавины)
    seen: std::collections::HashSet<u64>,
}

/// Что произошло при обработке конверта.
#[derive(Default)]
pub struct Outcome {
    /// Новые принятые сообщения чата.
    pub new_messages: Vec<ChatMessage>,
    /// Обновлённые пиры.
    pub new_peers: Vec<PeerInfo>,
    /// Что нужно переслать соседям / вернуть отправителю.
    pub outgoing: Vec<Envelope>,
    /// Группы, по которым не хватает родителей — надо запросить синк.
    pub needs_sync: Vec<GroupId>,
}

impl GossipEngine {
    pub fn new(identity: p2pchat_crypto::Identity, log: Log) -> Self {
        let peers = PeerTable::new(identity.peer_id().to_vec());
        Self { identity, log, peers, groups: Default::default(), seen: Default::default() }
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

    /// Создать сообщение и упаковать его в конверт для рассылки.
    /// Если для группы есть ключ — payload шифруется.
    pub fn post(&mut self, group: GroupId, text: String) -> Result<Envelope, CoreError> {
        let msg = self.log.append_local(group, self.peer_id(), text)?;
        self.seal_body_message(msg)
    }

    /// Создать новую группу: генерирует ключ и возвращает его для раздачи.
    pub fn create_group(&mut self, group: GroupId) -> [u8; 32] {
        let key = GroupKey::generate();
        let bytes = key.export();
        self.groups.insert(group, key);
        bytes
    }

    /// Вступить в группу, зная её ключ (получен по защищённому каналу).
    pub fn join_group(&mut self, group: GroupId, key: [u8; 32]) {
        self.groups.insert(group, GroupKey::from_bytes(key));
    }

    pub fn has_group_key(&self, group: &[u8]) -> bool {
        self.groups.contains_key(group)
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

    /// Разослать набор сообщений (каждое — своим конвертом, с шифрованием).
    fn emit_messages(&self, msgs: Vec<ChatMessage>, out: &mut Outcome) -> Result<(), CoreError> {
        for m in msgs {
            out.outgoing.push(self.seal_body_message(m)?);
        }
        Ok(())
    }

    /// Обработать входящий конверт.
    pub fn handle(&mut self, bytes: &[u8]) -> Result<Outcome, CoreError> {
        let env = Envelope::decode(bytes)?;
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
                self.emit_messages(resp.messages, &mut out)?;
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
                self.on_digest(d, &mut out)?;
            }
            Some(Body::SyncIds(s)) => {
                self.on_sync_ids(s, &mut out)?;
            }
            Some(Body::SyncFetch(f)) => {
                let messages: Vec<ChatMessage> = f
                    .ids
                    .iter()
                    .filter_map(|id| self.log.get(id).cloned())
                    .collect();
                self.emit_messages(messages, &mut out)?;
            }
            Some(Body::Encrypted(enc)) => {
                self.on_encrypted(enc, &mut out)?;
            }
            Some(Body::Presence(p)) => {
                self.on_presence(p);
            }
            None => {}
        }

        // после применения — распространяем новое дальше
        let forward: Vec<ChatMessage> = out.new_messages.clone();
        for m in forward {
            let env = self.seal_body_message(m)?;
            out.outgoing.push(env);
        }

        // если не хватает родителей — инициируем Merkle-синк по группе
        for g in out.new_messages.iter().map(|m| m.group_id.clone()) {
            if !self.log.missing_parents(&g).is_empty() && !out.needs_sync.contains(&g) {
                out.needs_sync.push(g.clone());
                let env = self.sync_digest(&g)?;
                out.outgoing.push(env);
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

    fn on_digest(&mut self, d: SyncDigest, out: &mut Outcome) -> Result<(), CoreError> {
        let local = MerkleTree::build(self.log.ids_for_group(&d.group_id));
        let remote_root: [u8; 32] = d.root.as_slice().try_into().unwrap_or([0u8; 32]);
        if local.root() == remote_root && local.len() as u64 == d.count {
            return Ok(());
        }
        // расходимся — шлём свой список id
        let ids = self.log.ids_for_group(&d.group_id);
        out.outgoing.push(self.seal(Gossip {
            body: Some(Body::SyncIds(SyncIds { group_id: d.group_id, ids })),
        })?);
        Ok(())
    }

    fn on_sync_ids(&mut self, s: SyncIds, out: &mut Outcome) -> Result<(), CoreError> {
        let local = MerkleTree::build(self.log.ids_for_group(&s.group_id));
        let remote = MerkleTree::build(s.ids);

        // отдаём то, чего нет у собеседника
        let to_send = local.missing_from(&remote);
        if !to_send.is_empty() {
            let messages: Vec<ChatMessage> = to_send
                .iter()
                .filter_map(|id| self.log.get(id).cloned())
                .collect();
            self.emit_messages(messages, out)?;
        }

        // просим то, чего нет у нас
        let to_fetch = remote.missing_from(&local);
        if !to_fetch.is_empty() {
            out.outgoing.push(self.seal(Gossip {
                body: Some(Body::SyncFetch(SyncFetch {
                    group_id: s.group_id,
                    ids: to_fetch,
                })),
            })?);
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
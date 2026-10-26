//! Anti-entropy: Merkle-digest и обмен недостающими сообщениями.
//!
//! Отдельная ответственность движка — сверка и догон логов между узлами.
//! Здесь только чистая логика синка; транспорт/подпись — в родительском модуле.

use crate::merkle::MerkleTree;
use crate::proto::gossip::Body;
use crate::proto::{
    ChatMessage, Envelope, Gossip, SyncDigest, SyncFetch, SyncIds, SyncRequest, SyncResponse,
};
use crate::CoreError;

use super::{GossipEngine, Outcome, Outgoing};

impl GossipEngine {
    /// Merkle-digest группы: корень + количество.
    pub fn sync_digest(&self, group: &[u8]) -> Result<Envelope, CoreError> {
        let tree = MerkleTree::build(self.log.ids_for_group(group));
        let d = SyncDigest {
            group_id: group.to_vec(),
            root: tree.root().to_vec(),
            count: tree.len() as u64,
        };
        self.seal(Gossip {
            body: Some(Body::SyncDigest(d)),
        })
    }

    /// Digest группы, адресованный конкретному пиру (для офлайн-догона).
    pub fn sync_digest_to(&self, group: &[u8], target: &[u8]) -> Result<Outgoing, CoreError> {
        let env = self.sync_digest(group)?;
        Ok(self.out_to(env, target.to_vec()))
    }

    /// Сформировать запрос синка по группе.
    pub fn sync_request(&self, group: &[u8]) -> Result<Envelope, CoreError> {
        let req = SyncRequest {
            group_id: group.to_vec(),
            have_heads: self.log.heads(group),
            lamport_watermark: self.log.lamport(),
        };
        self.seal(Gossip {
            body: Some(Body::SyncRequest(req)),
        })
    }

    /// Ответ на запрос синка: сообщения новее указанного времени.
    pub(crate) fn make_sync_response(&self, req: &SyncRequest) -> SyncResponse {
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

    /// Получили digest: если корни разошлись — отвечаем своим списком id.
    pub(crate) fn on_digest(
        &mut self,
        d: SyncDigest,
        origin: &[u8],
        out: &mut Outcome,
    ) -> Result<(), CoreError> {
        let local = MerkleTree::build(self.log.ids_for_group(&d.group_id));
        let remote_root: [u8; 32] = d.root.as_slice().try_into().unwrap_or([0u8; 32]);
        if local.root() == remote_root && local.len() as u64 == d.count {
            return Ok(());
        }
        let ids = self.log.ids_for_group(&d.group_id);
        let env = self.seal(Gossip {
            body: Some(Body::SyncIds(SyncIds {
                group_id: d.group_id,
                ids,
            })),
        })?;
        out.outgoing.push(self.out_to(env, origin.to_vec()));
        Ok(())
    }

    /// Получили список id: отдаём недостающее и просим своё.
    pub(crate) fn on_sync_ids(
        &mut self,
        s: SyncIds,
        origin: &[u8],
        out: &mut Outcome,
    ) -> Result<(), CoreError> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Log;
    use p2pchat_crypto::Identity;
    use prost::Message as _;

    #[test]
    fn request_and_response_cover_empty_and_nonempty() {
        let mut e = GossipEngine::new(Identity::generate(), Log::new());
        e.create_group(b"g".to_vec()).unwrap();
        e.post(b"g".to_vec(), "hi".into()).unwrap();

        let env = e.sync_request(b"g").unwrap();
        let g = Gossip::decode(env.payload.as_slice()).unwrap();
        let req = match g.body {
            Some(Body::SyncRequest(r)) => r,
            _ => panic!("not request"),
        };
        assert_eq!(req.group_id, b"g");
        assert_eq!(req.lamport_watermark, e.log().lamport());
        let mut old = req.clone();
        old.lamport_watermark = 0;
        let resp = e.make_sync_response(&old);
        assert_eq!(resp.messages.len(), 1);
        assert!(!resp.more);

        let empty = SyncRequest {
            group_id: b"other".to_vec(),
            have_heads: Vec::new(),
            lamport_watermark: 0,
        };
        assert!(e.make_sync_response(&empty).messages.is_empty());

        let out = e.sync_digest_to(b"g", b"peer").unwrap();
        assert_eq!(out.targets, vec![b"peer".to_vec()]);
    }
}

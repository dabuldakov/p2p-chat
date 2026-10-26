//! Реакции: постановка/снятие, приём LWW-событий и сводка для UI.

use super::*;

const MAX_REACTIONS_PER_USER: usize = 100;

/// Агрегированная реакция для UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactionSummary {
    pub emoji: String,
    pub count: u32,
    pub mine: bool,
}

impl GossipEngine {
    /// Поставить/снять свою реакцию (одна активная на сообщение).
    pub fn react(
        &mut self,
        group: &[u8],
        target_id: &[u8],
        emoji: &str,
        add: bool,
    ) -> Result<Option<Outgoing>, CoreError> {
        if !self.mls.has(group) {
            return Ok(None);
        }
        let Some(msg) = self.log.get(target_id) else {
            return Ok(None);
        };
        if msg.group_id != group {
            return Ok(None);
        }
        let emoji = emoji.trim();
        if emoji.is_empty() || emoji.len() > 64 {
            return Ok(None);
        }
        let me = self.peer_id();
        let mine = self.reactions.get(target_id).and_then(|m| m.get(&me));
        let cur = mine.and_then(|e| e.get(emoji)).copied();
        if add {
            if cur.is_some() {
                return Ok(None);
            }
            if mine.map(|e| e.len()).unwrap_or(0) >= MAX_REACTIONS_PER_USER {
                return Ok(None);
            }
        } else if cur.is_none() {
            return Ok(None);
        }
        let lamport = self.log.tick();
        let wt = crate::unix_now();
        if add {
            self.reactions
                .entry(target_id.to_vec())
                .or_default()
                .entry(me.clone())
                .or_default()
                .insert(emoji.to_string(), lamport);
            let _ = self.log.store_reaction(target_id, &me, emoji, lamport);
        } else {
            if let Some(m) = self.reactions.get_mut(target_id) {
                if let Some(by_peer) = m.get_mut(&me) {
                    by_peer.remove(emoji);
                    if by_peer.is_empty() {
                        m.remove(&me);
                    }
                }
                if m.is_empty() {
                    self.reactions.remove(target_id);
                }
            }
            let _ = self.log.remove_reaction(target_id, &me, emoji);
        }
        let ev = ReactionEvent {
            group_id: group.to_vec(),
            target_id: target_id.to_vec(),
            emoji: emoji.to_string(),
            add,
            author: me,
            lamport,
            wall_time: wt,
        };
        let env = self.seal(Gossip {
            body: Some(Body::Reaction(ev)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
    }

    /// Принять реакцию участника (LWW для пары сообщение+участник).
    pub(super) fn on_reaction(
        &mut self,
        ev: ReactionEvent,
        origin: &[u8],
    ) -> Result<(), CoreError> {
        if p2pchat_crypto::content_id(origin).to_vec() != ev.author {
            return Ok(());
        }
        if !self.mls.has(&ev.group_id) {
            return Ok(());
        }
        if !self
            .members(&ev.group_id)
            .iter()
            .any(|m| m.peer_id == ev.author)
        {
            return Ok(());
        }
        if let Some(m) = self.log.get(&ev.target_id) {
            if m.group_id != ev.group_id {
                return Ok(());
            }
        }
        let emoji = ev.emoji.trim();
        if emoji.is_empty() || emoji.len() > 64 {
            return Ok(());
        }
        let cur = self
            .reactions
            .get(&ev.target_id)
            .and_then(|m| m.get(&ev.author))
            .and_then(|e| e.get(emoji))
            .copied();
        if cur.is_some_and(|lamport| lamport > ev.lamport) {
            return Ok(());
        }
        if ev.add {
            self.reactions
                .entry(ev.target_id.clone())
                .or_default()
                .entry(ev.author.clone())
                .or_default()
                .insert(emoji.to_string(), ev.lamport);
            let _ = self
                .log
                .store_reaction(&ev.target_id, &ev.author, emoji, ev.lamport);
        } else {
            if cur.is_none() {
                return Ok(());
            }
            if let Some(m) = self.reactions.get_mut(&ev.target_id) {
                if let Some(by_peer) = m.get_mut(&ev.author) {
                    by_peer.remove(emoji);
                    if by_peer.is_empty() {
                        m.remove(&ev.author);
                    }
                }
                if m.is_empty() {
                    self.reactions.remove(&ev.target_id);
                }
            }
            let _ = self.log.remove_reaction(&ev.target_id, &ev.author, emoji);
        }
        Ok(())
    }

    /// Сводка реакций сообщения: количество по emoji и признак «моя».
    pub fn reactions_for(&self, target_id: &[u8], me: &[u8]) -> Vec<ReactionSummary> {
        let Some(map) = self.reactions.get(target_id) else {
            return Vec::new();
        };
        let mut counts: std::collections::HashMap<String, (u32, bool)> =
            std::collections::HashMap::new();
        for (peer, by_peer) in map {
            for emoji in by_peer.keys() {
                let e = counts.entry(emoji.clone()).or_default();
                e.0 += 1;
                if peer == me {
                    e.1 = true;
                }
            }
        }
        let mut out: Vec<ReactionSummary> = counts
            .into_iter()
            .map(|(emoji, (count, mine))| ReactionSummary { emoji, count, mine })
            .collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.emoji.cmp(&b.emoji)));
        out
    }
}

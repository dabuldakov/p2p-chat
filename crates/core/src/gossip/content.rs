//! Контент: связи, правки, удаления и выход участников.

use super::*;
use crate::proto::{DeleteEvent, EditEvent, LeaveEvent, Link, Links};

impl GossipEngine {
    /// Принять публикацию связей (эфемерно, LWW по времени автора).
    pub(super) fn on_links(&mut self, l: Links, origin: &[u8]) -> Result<(), CoreError> {
        // принимаем только от участника группы и с корректным peer_id
        let is_member = self
            .members(&l.group_id)
            .iter()
            .any(|m| m.public_key == origin);
        if !is_member || p2pchat_crypto::content_id(origin).to_vec() != l.peer_id {
            return Ok(());
        }
        let snap: Vec<(Vec<u8>, String)> = l
            .links
            .iter()
            .map(|k| (k.peer_id.clone(), k.kind.clone()))
            .collect();
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
                .map(|(p, k)| Link {
                    peer_id: p,
                    kind: k,
                })
                .collect(),
        };
        let snap: Vec<(Vec<u8>, String)> = msg
            .links
            .iter()
            .map(|k| (k.peer_id.clone(), k.kind.clone()))
            .collect();
        self.links
            .entry(group.to_vec())
            .or_default()
            .insert(me, (msg.time, snap));
        let env = self.seal(Gossip {
            body: Some(Body::Links(msg)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
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

    /// Отредактировать своё сообщение (только автор). Возвращает конверт.
    pub fn post_edit(
        &mut self,
        group: &[u8],
        target_id: &[u8],
        text: String,
    ) -> Result<Option<Outgoing>, CoreError> {
        if !self.mls.has(group) {
            return Ok(None);
        }
        let me = self.peer_id();
        let Some(m) = self.log.get(target_id) else {
            return Ok(None);
        };
        if m.author != me {
            return Ok(None); // править можно только свои
        }
        let lamport = self.log.tick();
        let wt = crate::unix_now();
        self.edits
            .insert(target_id.to_vec(), (text.clone(), lamport));
        let _ = self.log.store_edit(target_id, &text, lamport, wt);
        let ev = EditEvent {
            group_id: group.to_vec(),
            target_id: target_id.to_vec(),
            text,
            lamport,
            wall_time: wt,
            author: me,
        };
        let env = self.seal(Gossip {
            body: Some(Body::Edit(ev)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
    }

    /// Принять правку: только от автора исходного сообщения, LWW.
    pub(super) fn on_edit(&mut self, ev: EditEvent, origin: &[u8]) -> Result<(), CoreError> {
        if p2pchat_crypto::content_id(origin).to_vec() != ev.author {
            return Ok(());
        }
        let Some(m) = self.log.get(&ev.target_id) else {
            return Ok(());
        };
        if m.author != ev.author {
            return Ok(());
        }
        let cur = self.edits.get(&ev.target_id).map(|(_, l)| *l).unwrap_or(0);
        if ev.lamport >= cur {
            self.edits
                .insert(ev.target_id.clone(), (ev.text.clone(), ev.lamport));
            let _ = self
                .log
                .store_edit(&ev.target_id, &ev.text, ev.lamport, ev.wall_time);
        }
        Ok(())
    }

    /// Удалить своё сообщение (только автор). Возвращает конверт.
    pub fn post_delete(
        &mut self,
        group: &[u8],
        target_id: &[u8],
    ) -> Result<Option<Outgoing>, CoreError> {
        if !self.mls.has(group) {
            return Ok(None);
        }
        let me = self.peer_id();
        let Some(m) = self.log.get(target_id) else {
            return Ok(None);
        };
        if m.author != me {
            return Ok(None); // удалять можно только свои
        }
        let lamport = self.log.tick();
        let wt = crate::unix_now();
        self.deletes.insert(target_id.to_vec());
        let _ = self.log.store_delete(target_id);
        let ev = DeleteEvent {
            group_id: group.to_vec(),
            target_id: target_id.to_vec(),
            lamport,
            wall_time: wt,
            author: me,
        };
        let env = self.seal(Gossip {
            body: Some(Body::Delete(ev)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
    }

    /// Принять удаление: только от автора исходного сообщения.
    pub(super) fn on_delete(&mut self, ev: DeleteEvent, origin: &[u8]) -> Result<(), CoreError> {
        if p2pchat_crypto::content_id(origin).to_vec() != ev.author {
            return Ok(());
        }
        let Some(m) = self.log.get(&ev.target_id) else {
            return Ok(());
        };
        if m.author != ev.author {
            return Ok(());
        }
        if self.deletes.insert(ev.target_id.clone()) {
            let _ = self.log.store_delete(&ev.target_id);
        }
        Ok(())
    }

    /// Принять выход: уходящий должен быть участником; владелец делает Remove.
    pub(super) fn on_leave(
        &mut self,
        ev: LeaveEvent,
        origin: &[u8],
        out: &mut Outcome,
    ) -> Result<(), CoreError> {
        if p2pchat_crypto::content_id(origin).to_vec() != ev.author || ev.peer_id != ev.author {
            return Ok(());
        }
        if !self
            .members(&ev.group_id)
            .iter()
            .any(|m| m.peer_id == ev.peer_id)
        {
            return Ok(()); // не участник (или уже удалён)
        }

        // Если уходит владелец — принимаем назначенного нового владельца.
        let leaver_is_owner = self.owner_of(&ev.group_id).as_deref() == Some(ev.peer_id.as_slice());
        if leaver_is_owner && !ev.new_owner.is_empty() {
            let valid = self
                .members(&ev.group_id)
                .iter()
                .any(|m| m.peer_id == ev.new_owner && m.peer_id != ev.peer_id);
            if valid {
                self.owners
                    .insert(ev.group_id.clone(), ev.new_owner.clone());
                let _ = self
                    .log
                    .set_meta(&format!("owner:{}", hex(&ev.group_id)), &ev.new_owner);
                let _ = self.log.set_meta(
                    &format!("owner_alive:{}", hex(&ev.group_id)),
                    &self.now().to_le_bytes(),
                );
            }
        }

        if self.is_owner(&ev.group_id)? {
            let commits = self.remove_member_now(&ev.group_id, &ev.peer_id)?;
            out.outgoing.extend(commits);
        }
        Ok(())
    }

    pub fn is_deleted(&self, id: &[u8]) -> bool {
        self.deletes.contains(id)
    }

    /// Эффективный текст сообщения и признак правки.
    pub fn effective_text(&self, id: &[u8], base: &str) -> (String, bool) {
        match self.edits.get(id) {
            Some((t, _)) => (t.clone(), true),
            None => (base.to_string(), false),
        }
    }
}

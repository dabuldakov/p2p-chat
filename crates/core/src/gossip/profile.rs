//! Профили: ники, аватары пользователей и групп.

use super::*;
use crate::proto::{GroupProfile, ProfileEvent};

impl GossipEngine {
    pub fn set_nickname(&mut self, name: &str) -> Result<Option<Outgoing>, CoreError> {
        let pid = self.peer_id();
        let lamport = self.log.tick();
        self.nicknames
            .insert(pid.clone(), (name.to_string(), lamport));
        let avatar = self.avatars.get(&pid).cloned().unwrap_or_default();
        let av = if avatar.is_empty() {
            None
        } else {
            Some(avatar.as_slice())
        };
        let _ = self.log.store_profile(&pid, name, lamport, av);
        Ok(Some(self.profile_outgoing()?))
    }

    /// Задать свой аватар (маленькое изображение) и разослать профиль.
    pub fn set_avatar(&mut self, avatar: Vec<u8>) -> Result<Option<Outgoing>, CoreError> {
        let pid = self.peer_id();
        let lamport = self.log.tick();
        if avatar.is_empty() {
            self.avatars.remove(&pid);
        } else {
            self.avatars.insert(pid.clone(), avatar.clone());
        }
        let name = self
            .nicknames
            .get(&pid)
            .map(|(n, _)| n.clone())
            .unwrap_or_default();
        self.nicknames.insert(pid.clone(), (name.clone(), lamport));
        let av = if avatar.is_empty() {
            None
        } else {
            Some(avatar.as_slice())
        };
        let _ = self.log.store_profile(&pid, &name, lamport, av);
        Ok(Some(self.profile_outgoing()?))
    }

    /// Аватар участника.
    pub fn avatar_of(&self, peer_id: &[u8]) -> Option<Vec<u8>> {
        self.avatars.get(peer_id).cloned().filter(|a| !a.is_empty())
    }

    /// Все аватары участников.
    pub fn avatars(&self) -> Vec<(PeerId, Vec<u8>)> {
        self.avatars
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Аватар группы (из памяти или меты).
    pub fn group_avatar(&self, group: &[u8]) -> Option<Vec<u8>> {
        if let Some(a) = self.group_avatars.get(group) {
            if !a.is_empty() {
                return Some(a.clone());
            }
        }
        self.log
            .get_meta(&format!("gavatar:{}", hex(group)))
            .ok()
            .flatten()
            .filter(|a| !a.is_empty())
    }

    /// Сменить аватар группы (только владелец). Возвращает конверт для рассылки.
    pub fn set_group_avatar(
        &mut self,
        group: &[u8],
        avatar: Vec<u8>,
    ) -> Result<Option<Outgoing>, CoreError> {
        if !self.is_owner(group)? {
            return Ok(None);
        }
        if avatar.is_empty() {
            self.group_avatars.remove(group);
        } else {
            self.group_avatars.insert(group.to_vec(), avatar.clone());
        }
        self.send_group_profile(group, avatar)
    }

    /// Переименовать группу (только владелец). Рассылает профиль группы.
    pub fn set_group_name(
        &mut self,
        group: &[u8],
        name: String,
    ) -> Result<Option<Outgoing>, CoreError> {
        if !self.is_owner(group)? {
            return Ok(None);
        }
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let _ = self
            .log
            .set_meta(&format!("name:{}", hex(group)), name.as_bytes());
        let avatar = self.group_avatar(group).unwrap_or_default();
        self.send_group_profile(group, avatar)
    }

    /// Разослать профиль группы (аватар + имя) соседям, LWW по lamport.
    fn send_group_profile(
        &mut self,
        group: &[u8],
        avatar: Vec<u8>,
    ) -> Result<Option<Outgoing>, CoreError> {
        let lamport = self.log.tick();
        let me = self.peer_id();
        let name = self.group_name(group);
        let _ = self
            .log
            .set_meta(&format!("gavatar:{}", hex(group)), &avatar);
        let _ = self
            .log
            .set_meta(&format!("gavatar_l:{}", hex(group)), &lamport.to_le_bytes());
        let mut id_h = Vec::new();
        id_h.extend_from_slice(group);
        id_h.extend_from_slice(&lamport.to_be_bytes());
        id_h.extend_from_slice(&avatar);
        id_h.extend_from_slice(name.as_bytes());
        let ev = GroupProfile {
            group_id: group.to_vec(),
            avatar,
            lamport,
            author: me,
            id: p2pchat_crypto::content_id(&id_h).to_vec(),
            name,
        };
        let env = self.seal(Gossip {
            body: Some(Body::GroupProfile(ev)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
    }

    /// Принять аватар группы (только от владельца, LWW).
    pub(super) fn on_group_profile(
        &mut self,
        ev: GroupProfile,
        origin: &[u8],
    ) -> Result<(), CoreError> {
        // принимаем только от участника группы (ограничение «только владелец» —
        // на стороне отправителя в set_group_avatar)
        let is_member = self
            .members(&ev.group_id)
            .iter()
            .any(|m| m.public_key == origin);
        if !is_member {
            return Ok(());
        }
        let cur: u64 = self
            .log
            .get_meta(&format!("gavatar_l:{}", hex(&ev.group_id)))?
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
            .unwrap_or(0);
        if ev.lamport < cur {
            return Ok(());
        }
        self.group_avatars
            .insert(ev.group_id.clone(), ev.avatar.clone());
        let _ = self
            .log
            .set_meta(&format!("gavatar:{}", hex(&ev.group_id)), &ev.avatar);
        let _ = self.log.set_meta(
            &format!("gavatar_l:{}", hex(&ev.group_id)),
            &ev.lamport.to_le_bytes(),
        );
        if !ev.name.is_empty() {
            let _ = self
                .log
                .set_meta(&format!("name:{}", hex(&ev.group_id)), ev.name.as_bytes());
        }
        Ok(())
    }

    pub fn my_nickname(&self) -> Option<String> {
        self.nicknames.get(&self.peer_id()).map(|(n, _)| n.clone())
    }

    pub fn nickname_of(&self, peer_id: &[u8]) -> Option<String> {
        self.nicknames.get(peer_id).map(|(n, _)| n.clone())
    }

    pub fn all_nicknames(&self) -> Vec<(PeerId, String)> {
        self.nicknames
            .iter()
            .map(|(k, (n, _))| (k.clone(), n.clone()))
            .collect()
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
        let avatar = self.avatars.get(&pid).cloned().unwrap_or_default();
        let ev = crate::membership::profile_event(&pid, &name, lamport, &pid, &avatar);
        let env = self.seal(Gossip {
            body: Some(Body::Profile(ev)),
        })?;
        Ok(self.out_all(env))
    }

    pub(super) fn on_profile(&mut self, ev: ProfileEvent) -> Result<(), CoreError> {
        let wins = match self.nicknames.get(&ev.peer_id) {
            None => true,
            Some((_, l)) => ev.lamport > *l,
        };
        if wins {
            self.nicknames
                .insert(ev.peer_id.clone(), (ev.nickname.clone(), ev.lamport));
            if ev.avatar.is_empty() {
                self.avatars.remove(&ev.peer_id);
            } else {
                self.avatars.insert(ev.peer_id.clone(), ev.avatar.clone());
            }
            let av = if ev.avatar.is_empty() {
                None
            } else {
                Some(ev.avatar.as_slice())
            };
            let _ = self
                .log
                .store_profile(&ev.peer_id, &ev.nickname, ev.lamport, av);
        }
        Ok(())
    }
}

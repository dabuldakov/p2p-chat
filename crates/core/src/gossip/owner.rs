//! Выборы владельца группы: заявки, голоса, кворум, смена владельца.
//!
//! Протокол описан в `docs/owner-transfer.md`. События самодостаточны и
//! подписаны: заявка — кандидатом, голос — избирателем, итог — содержит
//! заявку и голоса, поэтому его может проверить и переслать любой участник.

use super::*;
use std::collections::HashMap;
mod rules;

const OWNER_CLOCK_SKEW: u64 = 600;
const OWNER_FAIL_COOLDOWN: u64 = 3 * 24 * 3600;
const OWNER_ACTIVE_SECS: u64 = 14 * 24 * 3600;

fn claim_sign_bytes(c: &OwnerClaim) -> Vec<u8> {
    let mut m = b"p2pchat-owner-claim-v1".to_vec();
    m.extend(&c.group_id);
    m.extend(&c.claimant);
    m.extend(&c.claimant_key);
    m.extend(&(c.trigger as u32).to_be_bytes());
    m.extend(&c.last_seen_owner.to_be_bytes());
    m.extend(&c.created_at.to_be_bytes());
    m.extend(&c.nonce);
    m
}

fn vote_sign_bytes(v: &OwnerVote) -> Vec<u8> {
    let mut m = b"p2pchat-owner-vote-v1".to_vec();
    m.extend(&v.group_id);
    m.extend(&v.claim_id);
    m.extend(&v.voter);
    m.extend(&v.voter_key);
    m.push(v.approve as u8);
    m.push(v.keep as u8);
    m.extend(&v.at.to_be_bytes());
    m
}

fn is_challenge(trigger: i32) -> bool {
    trigger == OwnerTrigger::Challenge as i32
}

fn is_auto(trigger: i32) -> bool {
    trigger == OwnerTrigger::Auto as i32
}

fn vote_start(e: &OwnerElection) -> u64 {
    if is_auto(e.trigger) {
        e.created_at
    } else {
        e.created_at + OWNER_APPLY_SECS
    }
}

fn vote_end(e: &OwnerElection) -> u64 {
    if is_auto(e.trigger) {
        e.created_at + OWNER_VOTE_SECS
    } else {
        e.created_at + OWNER_APPLY_SECS + OWNER_VOTE_SECS
    }
}

/// Снимок активных выборов для UI/FFI.
#[derive(Clone, Debug)]
pub struct OwnerElectionInfo {
    pub trigger: i32,
    pub created_at: u64,
    pub vote_start: u64,
    pub ends_at: u64,
    pub claims: Vec<OwnerClaim>,
    pub votes: Vec<OwnerVote>,
}

impl GossipEngine {
    /// Активные выборы группы (если есть).
    pub fn owner_election(&self, group: &[u8]) -> Option<OwnerElectionInfo> {
        let e = self.owner_elections.get(group)?;
        let mut claims: Vec<OwnerClaim> = e.claims.values().cloned().collect();
        claims.sort_by(|a, b| (a.created_at, &a.claim_id).cmp(&(b.created_at, &b.claim_id)));
        let mut votes: Vec<OwnerVote> = e.votes.values().cloned().collect();
        votes.sort_by(|a, b| a.voter.cmp(&b.voter));
        Some(OwnerElectionInfo {
            trigger: e.trigger,
            created_at: e.created_at,
            vote_start: vote_start(e),
            ends_at: vote_end(e),
            claims,
            votes,
        })
    }

    /// Есть ли незавершённые выборы (для заморозки состава группы).
    pub(super) fn owner_election_active(&self, group: &[u8]) -> bool {
        self.owner_elections.contains_key(group)
    }

    fn persist_election(&self, group: &[u8]) {
        let bytes = match self.owner_elections.get(group) {
            Some(e) => crate::proto::OwnerElectionState {
                group_id: group.to_vec(),
                trigger: e.trigger,
                created_at: e.created_at,
                claims: e.claims.values().cloned().collect(),
                votes: e.votes.values().cloned().collect(),
            }
            .encode_to_vec(),
            None => Vec::new(),
        };
        let _ = self
            .log
            .set_meta(&format!("owner_election:{}", hex(group)), &bytes);
    }

    /// Восстановить активные выборы из meta (вызывается при загрузке MLS).
    pub(super) fn load_owner_election(&mut self, group: &[u8]) {
        if self.owner_elections.contains_key(group) {
            return;
        }
        let Ok(Some(bytes)) = self.log.get_meta(&format!("owner_election:{}", hex(group))) else {
            return;
        };
        if bytes.is_empty() {
            return;
        }
        let Ok(st) = crate::proto::OwnerElectionState::decode(bytes.as_slice()) else {
            return;
        };
        if st.group_id != group {
            return;
        }
        self.owner_elections.insert(
            group.to_vec(),
            OwnerElection {
                trigger: st.trigger,
                created_at: st.created_at,
                claims: st
                    .claims
                    .into_iter()
                    .map(|c| (c.claim_id.clone(), c))
                    .collect(),
                votes: st.votes.into_iter().map(|v| (v.voter.clone(), v)).collect(),
            },
        );
    }

    /// Тестовые часы (для интеграционных тестов выборов). `None` — реальное время.
    #[doc(hidden)]
    pub fn set_test_now(&mut self, now: Option<u64>) {
        self.now_override = now;
    }

    pub(crate) fn now(&self) -> u64 {
        self.now_override.unwrap_or_else(crate::unix_now)
    }

    /// Номер эпохи владельца (растёт при каждой смене).
    pub fn owner_epoch(&self, group: &[u8]) -> u64 {
        self.log
            .get_meta(&format!("owner_epoch:{}", hex(group)))
            .ok()
            .flatten()
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
            .unwrap_or(0)
    }

    /// Когда последний раз начинался/завершался вызов владельца (unix; 0 — не было).
    pub fn owner_challenge_at(&self, group: &[u8]) -> u64 {
        self.log
            .get_meta(&format!("owner_challenge:{}", hex(group)))
            .ok()
            .flatten()
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
            .unwrap_or(0)
    }

    fn set_owner_challenge_at(&mut self, group: &[u8], at: u64) {
        if at > self.owner_challenge_at(group) {
            let _ = self.log.set_meta(
                &format!("owner_challenge:{}", hex(group)),
                &at.to_le_bytes(),
            );
        }
    }

    fn owner_fail_at(&self, group: &[u8]) -> u64 {
        self.log
            .get_meta(&format!("owner_fail:{}", hex(group)))
            .ok()
            .flatten()
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
            .unwrap_or(0)
    }

    fn set_owner_fail_at(&mut self, group: &[u8], at: u64) {
        let _ = self
            .log
            .set_meta(&format!("owner_fail:{}", hex(group)), &at.to_le_bytes());
    }

    /// Конверт с последним `OwnerChange` для догона офлайн-участников.
    pub fn owner_change_envelope(&self, group: &[u8]) -> Result<Option<Outgoing>, CoreError> {
        let Ok(Some(bytes)) = self.log.get_meta(&format!("owner_change:{}", hex(group))) else {
            return Ok(None);
        };
        if bytes.is_empty() {
            return Ok(None);
        }
        let Ok(change) = OwnerChange::decode(bytes.as_slice()) else {
            return Ok(None);
        };
        if change.group_id != group {
            return Ok(None);
        }
        let env = self.seal(Gossip {
            body: Some(Body::OwnerChange(change)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
    }

    /// Создать заявку на владение. `None`, если условия ещё не выполнены.
    pub fn create_owner_claim(
        &mut self,
        group: &[u8],
        trigger: OwnerTrigger,
    ) -> Result<Option<Outgoing>, CoreError> {
        let now = self.now();
        self.create_owner_claim_at(group, trigger, now)
    }

    pub fn create_owner_claim_at(
        &mut self,
        group: &[u8],
        trigger: OwnerTrigger,
        now: u64,
    ) -> Result<Option<Outgoing>, CoreError> {
        if !self.mls.has(group) {
            return Ok(None);
        }
        let me = self.peer_id();
        let Some(owner) = self.owner_of(group) else {
            return Ok(None);
        };
        if me == owner {
            return Ok(None);
        }
        let last = self.owner_last_seen(group);
        let trig = trigger as i32;
        match trigger {
            OwnerTrigger::Absent => {
                let fail = self.owner_fail_at(group);
                if fail != 0 && now < fail + OWNER_FAIL_COOLDOWN {
                    return Ok(None);
                }
                if last == 0 || now < last + OWNER_ABSENT_SECS {
                    return Ok(None);
                }
            }
            OwnerTrigger::Auto => {
                let fail = self.owner_fail_at(group);
                if fail != 0 && now < fail + OWNER_FAIL_COOLDOWN {
                    return Ok(None);
                }
                if last == 0 || now < last + OWNER_ABSENT_SECS + OWNER_APPLY_SECS {
                    return Ok(None);
                }
                if self.owner_successor(group).as_deref() != Some(me.as_slice()) {
                    return Ok(None);
                }
            }
            OwnerTrigger::Challenge => {
                let ch = self.owner_challenge_at(group);
                if ch != 0 && now < ch + OWNER_CHALLENGE_COOLDOWN {
                    return Ok(None);
                }
            }
        }
        let expired = self
            .owner_elections
            .get(group)
            .map(|e| now >= vote_end(e))
            .unwrap_or(true);
        if !expired {
            let e = self.owner_elections.get(group).expect("election");
            if e.trigger != trig
                || is_auto(trig)
                || now >= vote_start(e)
                || e.claims.values().any(|x| x.claimant == me)
            {
                return Ok(None);
            }
        } else {
            self.owner_elections.remove(group);
        }

        let public_key = self.public_key();
        let mut nonce = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce);
        let mut c = OwnerClaim {
            group_id: group.to_vec(),
            claim_id: Vec::new(),
            claimant: me,
            claimant_key: public_key,
            trigger: trig,
            last_seen_owner: last,
            created_at: now,
            nonce: nonce.to_vec(),
            sig: Vec::new(),
        };
        let signed = claim_sign_bytes(&c);
        c.claim_id = p2pchat_crypto::content_id(&signed).to_vec();
        c.sig = self.identity.sign(&signed);
        let claim_id = c.claim_id.clone();
        let created_at = c.created_at;

        if !self.owner_elections.contains_key(group) {
            self.owner_elections.insert(
                group.to_vec(),
                OwnerElection {
                    trigger: trig,
                    created_at,
                    claims: HashMap::new(),
                    votes: HashMap::new(),
                },
            );
        }
        if let Some(e) = self.owner_elections.get_mut(group) {
            e.claims.insert(claim_id, c.clone());
        }
        self.persist_election(group);
        let env = self.seal(Gossip {
            body: Some(Body::OwnerClaim(c)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
    }

    /// Проголосовать за заявку (`approve`) или оставить владельца (`keep`).
    pub fn owner_vote(
        &mut self,
        group: &[u8],
        claim_id: &[u8],
        approve: bool,
        keep: bool,
    ) -> Result<Option<Outgoing>, CoreError> {
        let now = self.now();
        self.owner_vote_at(group, claim_id, approve, keep, now)
    }

    pub fn owner_vote_at(
        &mut self,
        group: &[u8],
        claim_id: &[u8],
        approve: bool,
        keep: bool,
        now: u64,
    ) -> Result<Option<Outgoing>, CoreError> {
        let Some(e) = self.owner_elections.get(group).cloned() else {
            return Ok(None);
        };
        if now < vote_start(&e) || now >= vote_end(&e) {
            return Ok(None);
        }
        let me = self.peer_id();
        if !self
            .election_eligible(group, e.trigger)
            .iter()
            .any(|p| p == &me)
        {
            return Ok(None);
        }
        if keep && !is_challenge(e.trigger) {
            return Ok(None);
        }
        if !keep {
            let Some(c) = e.claims.get(claim_id) else {
                return Ok(None);
            };
            if c.claimant == me {
                return Ok(None);
            }
        }
        if e.votes.contains_key(&me) {
            return Ok(None);
        }
        let mut v = OwnerVote {
            group_id: group.to_vec(),
            claim_id: if keep { Vec::new() } else { claim_id.to_vec() },
            voter: me,
            voter_key: self.public_key(),
            approve,
            keep,
            at: now,
            sig: Vec::new(),
        };
        v.sig = self.identity.sign(&vote_sign_bytes(&v));
        let voter = v.voter.clone();
        if let Some(e) = self.owner_elections.get_mut(group) {
            e.votes.insert(voter, v.clone());
        }
        self.persist_election(group);
        let env = self.seal(Gossip {
            body: Some(Body::OwnerVote(v)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
    }

    /// Завершить выборы, если окно истекло. Возвращает `OwnerChange` для рассылки.
    pub fn finalize_owner_election(&mut self, group: &[u8]) -> Result<Option<Outgoing>, CoreError> {
        let now = self.now();
        self.finalize_owner_election_at(group, now)
    }

    pub fn finalize_owner_election_at(
        &mut self,
        group: &[u8],
        now: u64,
    ) -> Result<Option<Outgoing>, CoreError> {
        let Some(e) = self.owner_elections.get(group).cloned() else {
            return Ok(None);
        };
        if now < vote_end(&e) {
            return Ok(None);
        }
        if !is_challenge(e.trigger) {
            let last = self.owner_last_seen(group);
            if last > 0 && now < last + OWNER_ABSENT_SECS {
                self.owner_elections.remove(group);
                self.persist_election(group);
                return Ok(None);
            }
        }
        let Some((claim, votes)) = self.election_result(group, &e) else {
            if is_challenge(e.trigger) {
                self.set_owner_challenge_at(group, now);
            } else {
                self.set_owner_fail_at(group, now);
            }
            self.owner_elections.remove(group);
            self.persist_election(group);
            return Ok(None);
        };
        let epoch = self.owner_epoch(group) + 1;
        let winner = claim.claimant.clone();
        let candidates: Vec<OwnerClaim> = e.claims.values().cloned().collect();
        let change = OwnerChange {
            group_id: group.to_vec(),
            claim: Some(claim),
            votes,
            new_owner: winner,
            at: now,
            epoch,
            candidates,
        };
        self.apply_change(group, &change, now)?;
        let env = self.seal(Gossip {
            body: Some(Body::OwnerChange(change)),
        })?;
        Ok(Some(Outgoing {
            envelope: env,
            targets: self.neighbor_keys(group),
        }))
    }

    /// Принять заявку.
    pub fn on_owner_claim(&mut self, c: OwnerClaim, _origin: &[u8]) -> Result<(), CoreError> {
        if !self.claim_valid(&c, self.now(), true, false) {
            return Ok(());
        }
        let now = self.now();
        let expired = self
            .owner_elections
            .get(&c.group_id)
            .map(|e| now >= vote_end(e))
            .unwrap_or(true);
        if !expired {
            let e = self.owner_elections.get(&c.group_id).expect("election");
            if e.trigger != c.trigger
                || is_auto(c.trigger)
                || now >= vote_start(e)
                || e.claims.contains_key(&c.claim_id)
                || e.claims.values().any(|x| x.claimant == c.claimant)
            {
                return Ok(());
            }
        } else {
            self.owner_elections.remove(&c.group_id);
        }
        if !self.owner_elections.contains_key(&c.group_id) {
            self.owner_elections.insert(
                c.group_id.clone(),
                OwnerElection {
                    trigger: c.trigger,
                    created_at: c.created_at,
                    claims: HashMap::new(),
                    votes: HashMap::new(),
                },
            );
        }
        let group = c.group_id.clone();
        if let Some(e) = self.owner_elections.get_mut(&group) {
            e.claims.insert(c.claim_id.clone(), c);
        }
        self.persist_election(&group);
        Ok(())
    }

    /// Принять голос.
    pub fn on_owner_vote(&mut self, v: OwnerVote, _origin: &[u8]) -> Result<(), CoreError> {
        let now = self.now();
        let Some(e) = self.owner_elections.get(&v.group_id).cloned() else {
            return Ok(());
        };
        if now < vote_start(&e) || now >= vote_end(&e) {
            return Ok(());
        }
        if !self.vote_allowed(&v, &e, now, true) {
            return Ok(());
        }
        if e.votes.contains_key(&v.voter) {
            return Ok(());
        }
        let group = v.group_id.clone();
        if let Some(e) = self.owner_elections.get_mut(&group) {
            e.votes.insert(v.voter.clone(), v);
        }
        self.persist_election(&group);
        Ok(())
    }

    /// Принять итог выборов. `OwnerChange` самодостаточен: все заявки + подписи голосов.
    pub fn on_owner_change(&mut self, ch: OwnerChange, origin: &[u8]) -> Result<(), CoreError> {
        if !self
            .members(&ch.group_id)
            .iter()
            .any(|m| m.public_key == origin)
        {
            return Ok(());
        }
        let Some(winner_claim) = ch.claim.clone() else {
            return Ok(());
        };
        if winner_claim.group_id != ch.group_id {
            return Ok(());
        }
        let mut candidates = if ch.candidates.is_empty() {
            vec![winner_claim.clone()]
        } else {
            ch.candidates.clone()
        };
        if !candidates
            .iter()
            .any(|c| c.claim_id == winner_claim.claim_id)
        {
            return Ok(());
        }
        let trigger = winner_claim.trigger;
        let now = self.now();
        let cur_epoch = self.owner_epoch(&ch.group_id);
        if ch.epoch < cur_epoch {
            return Ok(());
        }
        if ch.epoch == cur_epoch {
            // Один epoch мог дать два OwnerChange в partition. Выбираем
            // детерминированно лучший: больше голосов, затем порядок наследников.
            let Ok(Some(bytes)) = self
                .log
                .get_meta(&format!("owner_change:{}", hex(&ch.group_id)))
            else {
                return Ok(());
            };
            let Ok(cur) = OwnerChange::decode(bytes.as_slice()) else {
                return Ok(());
            };
            let (Some(new_rank), Some(cur_rank)) = (self.change_rank(&ch), self.change_rank(&cur))
            else {
                return Ok(());
            };
            if new_rank <= cur_rank {
                return Ok(());
            }
        }
        if self.owner_of(&ch.group_id).as_deref() == Some(ch.new_owner.as_slice()) {
            return Ok(()); // повтор/no-op
        }
        candidates.sort_by(|a, b| a.claim_id.cmp(&b.claim_id));
        let mut claims: HashMap<MessageId, OwnerClaim> = HashMap::new();
        for c in &candidates {
            if c.group_id != ch.group_id || c.trigger != trigger {
                return Ok(());
            }
            if !self.claim_valid(c, now, false, true) {
                return Ok(());
            }
            if claims.insert(c.claim_id.clone(), c.clone()).is_some() {
                return Ok(());
            }
        }
        if !claims.contains_key(&winner_claim.claim_id) || ch.new_owner != winner_claim.claimant {
            return Ok(());
        }
        let eligible = self.election_eligible(&ch.group_id, trigger);
        if eligible.is_empty() {
            return Ok(());
        }
        let mut seen: std::collections::HashSet<PeerId> = std::collections::HashSet::new();
        for c in claims.values() {
            if !eligible.iter().any(|p| p == &c.claimant) {
                return Ok(());
            }
            if !seen.insert(c.claimant.clone()) {
                return Ok(());
            }
        }
        let mut votes: HashMap<PeerId, OwnerVote> = HashMap::new();
        for v in &ch.votes {
            if !self.vote_struct_valid(v, now, false) {
                return Ok(());
            }
            if !eligible.iter().any(|p| p == &v.voter) {
                return Ok(());
            }
            if !seen.insert(v.voter.clone()) {
                return Ok(());
            }
            if v.keep {
                if !is_challenge(trigger) {
                    return Ok(());
                }
            } else {
                let Some(c) = claims.get(&v.claim_id) else {
                    return Ok(());
                };
                if c.claimant == v.voter {
                    return Ok(());
                }
            }
            votes.insert(v.voter.clone(), v.clone());
        }
        let Some(winner) = self.resolve_election(&ch.group_id, trigger, &claims, &votes) else {
            return Ok(());
        };
        if winner.claim_id != winner_claim.claim_id || winner.claimant != ch.new_owner {
            return Ok(());
        }
        self.apply_change(&ch.group_id, &ch, now)?;
        Ok(())
    }

    fn apply_change(&mut self, group: &[u8], ch: &OwnerChange, now: u64) -> Result<(), CoreError> {
        self.owners.insert(group.to_vec(), ch.new_owner.clone());
        let _ = self
            .log
            .set_meta(&format!("owner:{}", hex(group)), &ch.new_owner);
        let _ = self
            .log
            .set_meta(&format!("owner_alive:{}", hex(group)), &now.to_le_bytes());
        let _ = self.log.set_meta(
            &format!("owner_epoch:{}", hex(group)),
            &ch.epoch.to_le_bytes(),
        );
        let _ = self
            .log
            .set_meta(&format!("owner_change:{}", hex(group)), &ch.encode_to_vec());
        if let Some(claim) = &ch.claim {
            if is_challenge(claim.trigger) {
                self.set_owner_challenge_at(group, ch.at);
            }
        }
        self.set_owner_fail_at(group, 0);
        self.owner_elections.remove(group);
        self.persist_election(group);
        Ok(())
    }
}

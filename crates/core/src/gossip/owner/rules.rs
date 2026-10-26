//! Правила выборов: допуск, проверка подписей, кворум и разрешение ничьих.

use super::super::*;
use super::{
    claim_sign_bytes, is_auto, is_challenge, vote_sign_bytes, OWNER_ABSENT_SECS, OWNER_ACTIVE_SECS,
    OWNER_APPLY_SECS, OWNER_CLOCK_SKEW, OWNER_FAIL_COOLDOWN,
};
use std::collections::HashMap;

impl GossipEngine {
    fn member_active(&self, peer: &[u8]) -> bool {
        if peer == self.peer_id() {
            return true;
        }
        let now = self.now();
        self.peers
            .get(peer)
            .map(|p| p.last_seen > 0 && now.saturating_sub(p.last_seen) <= OWNER_ACTIVE_SECS)
            .unwrap_or(false)
    }

    /// Первый по peer_id не-владелец, активный последние 14 дней.
    pub(super) fn owner_successor(&self, group: &[u8]) -> Option<PeerId> {
        let owner = self.owner_of(group);
        self.members(group)
            .into_iter()
            .filter(|m| Some(&m.peer_id) != owner.as_ref())
            .find(|m| self.member_active(&m.peer_id))
            .map(|m| m.peer_id)
    }

    pub(super) fn election_eligible(&self, group: &[u8], trigger: i32) -> Vec<PeerId> {
        let owner = self.owner_of(group);
        let mut v: Vec<PeerId> = self
            .members(group)
            .into_iter()
            .filter(|m| is_challenge(trigger) || Some(&m.peer_id) != owner.as_ref())
            .map(|m| m.peer_id)
            .collect();
        v.sort();
        v
    }

    /// Проверка заявки. `fresh` — принимать только свежие (при онлайн-получении).
    pub(super) fn claim_valid(
        &self,
        c: &OwnerClaim,
        now: u64,
        fresh: bool,
        allow_owner: bool,
    ) -> bool {
        if c.group_id.is_empty() || !self.mls.has(&c.group_id) {
            return false;
        }
        if c.claimant.len() != 32
            || c.claimant_key.len() != 32
            || c.claim_id.len() != 32
            || c.sig.len() != 64
        {
            return false;
        }
        if p2pchat_crypto::content_id(&c.claimant_key).to_vec() != c.claimant {
            return false;
        }
        let signed = claim_sign_bytes(c);
        if p2pchat_crypto::content_id(&signed).to_vec() != c.claim_id {
            return false;
        }
        if p2pchat_crypto::verify_bytes(&c.claimant_key, &signed, &c.sig).is_err() {
            return false;
        }
        let Some(owner) = self.owner_of(&c.group_id) else {
            return false;
        };
        if !allow_owner && c.claimant == owner {
            return false;
        }
        if !self
            .members(&c.group_id)
            .iter()
            .any(|m| m.peer_id == c.claimant && m.public_key == c.claimant_key)
        {
            return false;
        }
        if fresh && now.abs_diff(c.created_at) > OWNER_CLOCK_SKEW {
            return false;
        }
        if c.trigger == OwnerTrigger::Absent as i32 || is_auto(c.trigger) {
            if fresh {
                let fail = self.owner_fail_at(&c.group_id);
                if fail != 0 && now < fail + OWNER_FAIL_COOLDOWN {
                    return false;
                }
            }
            if c.last_seen_owner > c.created_at + OWNER_CLOCK_SKEW {
                return false;
            }
            let local = self.owner_last_seen(&c.group_id);
            let last = if fresh && local > c.last_seen_owner {
                local
            } else {
                c.last_seen_owner
            };
            let since = if fresh { now } else { c.created_at };
            if last == 0 || since < last + OWNER_ABSENT_SECS {
                return false;
            }
            if is_auto(c.trigger) && since < last + OWNER_ABSENT_SECS + OWNER_APPLY_SECS {
                // Активность заявителя доказывается самим фактом отправки заявки.
                // Локальный выбор наследника (с фильтром 14 дней) — на стороне клиента.
                return false;
            }
        } else if !is_challenge(c.trigger) {
            return false;
        }
        true
    }

    pub(super) fn vote_struct_valid(&self, v: &OwnerVote, now: u64, fresh: bool) -> bool {
        if v.group_id.is_empty() || !self.mls.has(&v.group_id) {
            return false;
        }
        if v.voter.len() != 32
            || v.voter_key.len() != 32
            || v.sig.len() != 64
            || v.claim_id.len() != if v.keep { 0 } else { 32 }
        {
            return false;
        }
        if p2pchat_crypto::content_id(&v.voter_key).to_vec() != v.voter {
            return false;
        }
        if p2pchat_crypto::verify_bytes(&v.voter_key, &vote_sign_bytes(v), &v.sig).is_err() {
            return false;
        }
        if fresh && now.abs_diff(v.at) > OWNER_CLOCK_SKEW {
            return false;
        }
        if !self
            .members(&v.group_id)
            .iter()
            .any(|m| m.peer_id == v.voter && m.public_key == v.voter_key)
        {
            return false;
        }
        true
    }

    pub(super) fn vote_allowed(
        &self,
        v: &OwnerVote,
        e: &OwnerElection,
        now: u64,
        fresh: bool,
    ) -> bool {
        if !self.vote_struct_valid(v, now, fresh) {
            return false;
        }
        if !self
            .election_eligible(&v.group_id, e.trigger)
            .iter()
            .any(|p| p == &v.voter)
        {
            return false;
        }
        if v.keep && !is_challenge(e.trigger) {
            return false;
        }
        if !v.keep {
            let Some(c) = e.claims.get(&v.claim_id) else {
                return false;
            };
            if c.claimant == v.voter {
                return false;
            }
        }
        true
    }

    /// Подсчёт выборов: кворум, большинство, ничьи. Возвращает победителя.
    pub(super) fn resolve_election(
        &self,
        group: &[u8],
        trigger: i32,
        claims: &HashMap<MessageId, OwnerClaim>,
        votes: &HashMap<PeerId, OwnerVote>,
    ) -> Option<OwnerClaim> {
        if claims.is_empty() {
            return None;
        }
        let eligible = self.election_eligible(group, trigger);
        if eligible.is_empty() {
            return None;
        }
        let quorum = eligible.len() / 2 + 1;
        let mut participation = 0usize;
        let mut objections = 0usize;
        let mut keep_votes = 0usize;
        let mut approvals: HashMap<MessageId, usize> =
            claims.keys().map(|id| (id.clone(), 1)).collect();
        let mut rejects: HashMap<MessageId, usize> = HashMap::new();
        for voter in &eligible {
            if claims.values().any(|c| &c.claimant == voter) {
                participation += 1;
                continue;
            }
            let Some(v) = votes.get(voter) else {
                continue;
            };
            if v.keep {
                keep_votes += 1;
            } else if v.approve {
                *approvals.entry(v.claim_id.clone()).or_default() += 1;
            } else {
                *rejects.entry(v.claim_id.clone()).or_default() += 1;
                objections += 1;
            }
            participation += 1;
        }
        if is_auto(trigger) {
            // Подтверждение: возражения >50% не-владельцев отменяют.
            if objections * 2 > eligible.len() {
                return None;
            }
        } else if participation < quorum {
            return None;
        }
        let mut best: Option<(&OwnerClaim, usize)> = None;
        for c in claims.values() {
            let n = approvals.get(&c.claim_id).copied().unwrap_or(0);
            let replace = match best {
                None => true,
                Some((bc, bn)) => n > bn || (n == bn && self.claim_tie_break(c, bc, group)),
            };
            if replace {
                best = Some((c, n));
            }
        }
        let (winner, winner_votes) = best?;
        if is_challenge(trigger) {
            let total = self.members(group).len();
            if winner_votes * 2 <= total || winner_votes <= keep_votes {
                return None;
            }
        } else if !is_auto(trigger)
            && winner_votes <= rejects.get(&winner.claim_id).copied().unwrap_or(0)
        {
            return None;
        }
        Some(winner.clone())
    }

    /// Итог активных выборов для рассылки.
    pub(super) fn election_result(
        &self,
        group: &[u8],
        e: &OwnerElection,
    ) -> Option<(OwnerClaim, Vec<OwnerVote>)> {
        let winner = self.resolve_election(group, e.trigger, &e.claims, &e.votes)?;
        let votes: Vec<OwnerVote> = e.votes.values().cloned().collect();
        Some((winner, votes))
    }

    /// Ранг конфликтующего OwnerChange с тем же epoch.
    pub(super) fn change_rank(
        &self,
        ch: &OwnerChange,
    ) -> Option<(usize, std::cmp::Reverse<usize>, PeerId, MessageId)> {
        let claim = ch.claim.as_ref()?;
        let votes = 1 + ch
            .votes
            .iter()
            .filter(|v| !v.keep && v.approve && v.claim_id == claim.claim_id)
            .count();
        let order = self.claimant_order_index(&ch.group_id, &claim.claimant);
        Some((
            votes,
            std::cmp::Reverse(order),
            ch.new_owner.clone(),
            claim.claim_id.clone(),
        ))
    }

    fn claimant_order_index(&self, group: &[u8], peer: &[u8]) -> usize {
        let owner = self.owner_of(group);
        let mut order: Vec<PeerId> = self
            .members(group)
            .into_iter()
            .filter(|m| Some(&m.peer_id) != owner.as_ref())
            .map(|m| m.peer_id)
            .collect();
        order.sort();
        order.iter().position(|p| p == peer).unwrap_or(usize::MAX)
    }

    /// При равенстве голосов выигрывает тот, кто раньше в порядке преемственности.
    fn claim_tie_break(&self, a: &OwnerClaim, b: &OwnerClaim, group: &[u8]) -> bool {
        let owner = self.owner_of(group);
        let mut order: Vec<PeerId> = self
            .members(group)
            .into_iter()
            .filter(|m| Some(&m.peer_id) != owner.as_ref())
            .map(|m| m.peer_id)
            .collect();
        order.sort();
        let ai = order
            .iter()
            .position(|p| p == &a.claimant)
            .unwrap_or(usize::MAX);
        let bi = order
            .iter()
            .position(|p| p == &b.claimant)
            .unwrap_or(usize::MAX);
        (ai, &a.claimant) < (bi, &b.claimant)
    }
}

package com.p2pchat.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OwnerUiLogicTest {
    @Test
    fun actions_visible_for_every_non_owner() {
        assertTrue(ownerActionsVisible(false))
        assertFalse(ownerActionsVisible(true))
    }

    @Test
    fun actions_disabled_during_election() {
        assertTrue(ownerActionEnabled(true, false))
        assertFalse(ownerActionEnabled(true, true))
        assertFalse(ownerActionEnabled(false, false))
    }

    @Test
    fun twoPersonChallengeVotesImmediately() {
        assertTrue(electionVotingOpen(trigger = 1, memberCount = 2, nowSec = 100u, voteStart = 1000u, endsAt = 2000u))
        assertFalse(electionVotingOpen(trigger = 0, memberCount = 2, nowSec = 100u, voteStart = 1000u, endsAt = 2000u))
        assertFalse(electionVotingOpen(trigger = 1, memberCount = 3, nowSec = 100u, voteStart = 1000u, endsAt = 2000u))
    }

    @Test
    fun votingStopsAtDeadline() {
        assertTrue(electionVotingOpen(1, 3, 1500u, 1000u, 2000u))
        assertFalse(electionVotingOpen(1, 3, 2000u, 1000u, 2000u))
    }

    @Test
    fun hint_only_for_collapsed_non_owner() {
        assertTrue(ownerManagementHintVisible(false, false))
        assertFalse(ownerManagementHintVisible(false, true))
        assertFalse(ownerManagementHintVisible(true, false))
    }
}

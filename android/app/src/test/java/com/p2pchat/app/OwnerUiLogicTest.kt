package com.p2pchat.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OwnerUiLogicTest {
    @Test
    fun actions_only_for_non_owner_without_election() {
        assertTrue(ownerActionsVisible(false, false))
        assertFalse(ownerActionsVisible(true, false))
        assertFalse(ownerActionsVisible(false, true))
        assertFalse(ownerActionsVisible(true, true))
    }

    @Test
    fun hint_only_when_actions_hidden_by_collapsed_card() {
        assertTrue(ownerManagementHintVisible(false, false, false))
        assertFalse(ownerManagementHintVisible(false, false, true))
        assertFalse(ownerManagementHintVisible(true, false, false))
        assertFalse(ownerManagementHintVisible(false, true, false))
    }
}

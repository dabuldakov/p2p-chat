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
    fun hint_only_for_collapsed_non_owner() {
        assertTrue(ownerManagementHintVisible(false, false))
        assertFalse(ownerManagementHintVisible(false, true))
        assertFalse(ownerManagementHintVisible(true, false))
    }
}

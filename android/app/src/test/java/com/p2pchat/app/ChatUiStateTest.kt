package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChatUiStateTest {

    @Test
    fun defaultsHaveNoChatData() {
        val state = ChatUiState()

        assertTrue(state.groupInfos.isEmpty())
        assertTrue(state.messages.isEmpty())
        assertTrue(state.members.isEmpty())
        assertTrue(state.nicknames.isEmpty())
        assertTrue(state.peerKinds.isEmpty())
        assertEquals(0, state.groupConnected)
        assertEquals(0, state.groupNeighbors)
        assertNull(state.currentGroup)
        assertNull(state.ownerElection)
    }

    @Test
    fun copyChangesOnlyRequestedSection() {
        val original = ChatUiState(
            currentGroupName = "Team",
            myPeerId = "peer-a",
            status = "direct",
        )

        val changed = original.copy(isOwner = true, groupNeighbors = 3)

        assertEquals("Team", changed.currentGroupName)
        assertEquals("peer-a", changed.myPeerId)
        assertEquals("direct", changed.status)
        assertTrue(changed.isOwner)
        assertEquals(3, changed.groupNeighbors)
        assertEquals(false, original.isOwner)
    }
}

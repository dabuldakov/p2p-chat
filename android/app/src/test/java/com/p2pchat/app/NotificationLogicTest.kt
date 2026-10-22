package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NotificationLogicTest {

    private fun gi(idHex: String, name: String) = GroupInfo(hexToBytes(idHex)!!, name)

    @Test
    fun unreadGroupsFiltersAndKeepsOrder() {
        val a = gi("aa", "A")
        val b = gi("bb", "B")
        val c = gi("cc", "C")
        val r = unreadGroups(listOf(a, b, c), mapOf("aa" to 0uL, "bb" to 3uL, "cc" to 1uL))
        assertEquals(listOf("B", "C"), r.map { it.name })
        // На бейдже — число непрочитанных ГРУПП, не сообщений.
        assertEquals(2, r.size)
    }

    @Test
    fun unreadGroupsEmpty() {
        assertTrue(unreadGroups(emptyList(), emptyMap()).isEmpty())
        assertTrue(unreadGroups(listOf(gi("aa", "A")), emptyMap()).isEmpty())
    }

    @Test
    fun notificationTextFallsBack() {
        assertEquals("hi", notificationText("hi", "new"))
        assertEquals("new", notificationText(null, "new"))
        assertEquals("new", notificationText("   ", "new"))
    }

    @Test
    fun shouldMarkReadOnlyWhenVisibleAndOpen() {
        assertTrue(shouldMarkRead(appVisible = true, groupOpen = true))
        assertFalse(shouldMarkRead(appVisible = false, groupOpen = true))
        assertFalse(shouldMarkRead(appVisible = true, groupOpen = false))
        assertFalse(shouldMarkRead(appVisible = false, groupOpen = false))
    }
}

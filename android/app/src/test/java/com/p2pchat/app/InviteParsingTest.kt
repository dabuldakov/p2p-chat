package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class InviteParsingTest {

    private val name = "Team Chat"
    private val invite = "p2pchat://join?g=0102&m=${name.toByteArray().toHex()}&s=sig"

    @Test
    fun extractsInviteAndDecodedGroupName() {
        assertEquals(invite to name, extractInvite("  $invite  "))
        assertEquals(invite to name, extractInvite("$invite\n"))
    }

    @Test
    fun rejectsInviteMixedWithTextOrWithoutName() {
        assertNull(extractInvite("hello $invite"))
        assertNull(extractInvite("p2pchat://join?g=1"))
        assertNull(extractInvite("https://example.test"))
    }

    @Test
    fun singleUrlOnlyWhenMessageIsExactlyOneLink() {
        assertEquals("https://example.test/a", singleUrl("https://example.test/a"))
        assertNull(singleUrl("https://example.test/a text"))
        assertNull(singleUrl("two https://example.test/a"))
    }
}

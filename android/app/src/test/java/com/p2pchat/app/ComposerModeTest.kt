package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Test

class ComposerModeTest {
    @Test
    fun cycles_text_voice_video() {
        assertEquals(SendMode.VOICE, nextSendMode(SendMode.TEXT))
        assertEquals(SendMode.VIDEO, nextSendMode(SendMode.VOICE))
        assertEquals(SendMode.TEXT, nextSendMode(SendMode.VIDEO))
    }

    @Test
    fun text_input_overrides_mode() {
        assertEquals(SendMode.TEXT, effectiveSendMode("hello", SendMode.VOICE))
        assertEquals(SendMode.VIDEO, effectiveSendMode("", SendMode.VIDEO))
    }
}

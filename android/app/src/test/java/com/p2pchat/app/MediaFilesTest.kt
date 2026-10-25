package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Test

class MediaFilesTest {
    @Test
    fun mime_extensions() {
        assertEquals("jpg", MediaFiles.extension("image/jpeg"))
        assertEquals("mp4", MediaFiles.extension("video/mp4"))
        assertEquals("m4a", MediaFiles.extension("audio/mp4"))
        assertEquals("mp3", MediaFiles.extension("audio/mpeg"))
        assertEquals("bin", MediaFiles.extension("application/octet-stream"))
    }

    @Test
    fun sha256_is_hex_and_stable() {
        assertEquals(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            MediaFiles.sha256("hello".toByteArray()),
        )
    }
}

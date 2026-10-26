package com.p2pchat.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class VersionTest {

    @Test
    fun newer_versions() {
        assertTrue(isNewerVersion("2.1.1", "2.1.0"))
        assertTrue(isNewerVersion("3.0.0", "2.9.9"))
        assertTrue(isNewerVersion("2.1", "2.0.9"))
    }

    @Test
    fun not_newer() {
        assertFalse(isNewerVersion("2.1.0", "2.1.0"))
        assertFalse(isNewerVersion("2.0.9", "2.1.0"))
        assertFalse(isNewerVersion("1.0", "1.0.0"))
    }
}

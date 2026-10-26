package com.p2pchat.app

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class UpdateControllerTest {
    private val minute = 60_000L
    private val now = 10 * 60 * minute

    @Test
    fun checksImmediatelyAfterStartAndWhenForced() {
        assertTrue(shouldCheckUpdate(now, lastCheck = 0L, checkFailed = false, force = false))
        assertTrue(shouldCheckUpdate(now, lastCheck = now - minute, checkFailed = false, force = true))
    }

    @Test
    fun successfulCheckIsThrottledToThirtyMinutes() {
        assertFalse(shouldCheckUpdate(now, now - 29 * minute, checkFailed = false, force = false))
        assertTrue(shouldCheckUpdate(now, now - 30 * minute, checkFailed = false, force = false))
    }

    @Test
    fun failedCheckRetriesAfterFiveMinutes() {
        assertFalse(shouldCheckUpdate(now, now - 4 * minute, checkFailed = true, force = false))
        assertTrue(shouldCheckUpdate(now, now - 5 * minute, checkFailed = true, force = false))
    }
}

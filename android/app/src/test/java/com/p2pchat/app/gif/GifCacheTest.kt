package com.p2pchat.app.gif

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class GifCacheTest {

    @get:Rule
    val tmp = TemporaryFolder()

    private var clock = 0L
    private fun cache(file: File = File(tmp.root, "c.json")) = GifCache(file) { clock }

    private fun gif(id: String) =
        Gif(providerId = id, url = "u$id", previewUrl = "p$id", mp4Url = null, width = 1, height = 1)

    @Test
    fun put_and_get() {
        val c = cache()
        c.put("k", listOf(gif("a")))
        assertEquals(1, c.getStale("k")?.size)
    }

    @Test
    fun missing_key_returns_null() {
        assertNull(cache().getStale("nope"))
    }

    @Test
    fun fresh_honors_ttl() {
        val c = cache()
        c.put("k", listOf(gif("a")))
        assertNotNull(c.getFresh("k", ttlMs = 1000))
        clock = 1001
        assertNull(c.getFresh("k", ttlMs = 1000))
        assertNotNull(c.getStale("k"))
    }

    @Test
    fun search_key_is_normalized() {
        assertEquals(GifCache.searchKey(" Cat "), GifCache.searchKey("cat"))
    }

    @Test
    fun persists_across_instances() {
        val file = File(tmp.root, "p.json")
        cache(file).put("k", listOf(gif("a")))
        assertEquals(1, cache(file).getStale("k")?.size)
    }

    @Test
    fun clear_removes_everything() {
        val c = cache()
        c.put("k", listOf(gif("a")))
        c.clear()
        assertNull(c.getStale("k"))
    }
}

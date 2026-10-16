package com.p2pchat.app.gif

import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class GifRepositoryTest {

    @get:Rule
    val tmp = TemporaryFolder()

    private lateinit var server: MockWebServer

    @Before
    fun setUp() {
        server = MockWebServer()
        server.start()
    }

    @After
    fun tearDown() {
        server.shutdown()
    }

    private fun repo(freshTtlMs: Long = 100_000): GifRepository {
        val api = KlipyApi("testkey", server.url("/").toString().trimEnd('/'), OkHttpClient())
        return GifRepository(
            api = api,
            cache = GifCache(File(tmp.root, "c.json")),
            mediaCache = GifMediaCache(File(tmp.root, "m.json")),
            pageSize = 2,
            freshTtlMs = freshTtlMs,
        )
    }

    private fun gifJson(id: Int) =
        """{"id":$id,"slug":"$id","title":"t","file":{"hd":{"webp":{"url":"https://x/$id.webp","width":220,"height":230,"size":1},"mp4":{"url":"https://x/$id.mp4","width":220,"height":230,"size":1}}}}"""

    private fun searchBody(ids: List<Int>) =
        """{"result":true,"data":{"data":[${ids.joinToString(",") { gifJson(it) }}],"current_page":1,"per_page":2,"has_next":false}}"""

    private fun byIdBody(id: Int) = """{"result":true,"data":${gifJson(id)}}"""

    @Test
    fun second_call_served_from_cache() = runTest {
        server.enqueue(MockResponse().setBody(searchBody(listOf(1, 2))))
        server.enqueue(MockResponse().setBody(searchBody(listOf(3, 4)))) // запасной, если кэш не сработает
        val r = repo()
        assertEquals(2, r.search("cat", 0).size)
        assertEquals(2, r.search("cat", 0).size)
        assertEquals(1, server.requestCount)
    }

    @Test
    fun blank_query_hits_trending() = runTest {
        server.enqueue(MockResponse().setBody(searchBody(listOf(1))))
        repo().search("", 0)
        val path = server.takeRequest().path!!
        assertTrue(path.contains("/gifs/trending"))
        assertTrue(path.contains("/api/v1/testkey/"))
    }

    @Test
    fun resolve_by_id_caches_media() = runTest {
        server.enqueue(MockResponse().setBody(byIdBody(7)))
        server.enqueue(MockResponse().setBody(byIdBody(7)))
        val r = repo()
        assertEquals("7", r.resolve("7")?.providerId)
        assertNotNull(r.resolve("7"))
        assertEquals(1, server.requestCount)
    }

    @Test
    fun network_error_falls_back_to_stale() = runTest {
        val cache = File(tmp.root, "shared.json")
        val media = File(tmp.root, "shared-media.json")

        val r1 = GifRepository(
            KlipyApi("testkey", server.url("/").toString().trimEnd('/'), OkHttpClient()),
            GifCache(cache), GifMediaCache(media), pageSize = 2, freshTtlMs = 100_000,
        )
        server.enqueue(MockResponse().setBody(searchBody(listOf(1, 2))))
        assertEquals(2, r1.search("dog", 0).size)

        // новый репозиторий с тем же кэшем, но TTL=0 (форсим сеть) и сервер отвечает 500
        val r2 = GifRepository(
            KlipyApi("testkey", server.url("/").toString().trimEnd('/'), OkHttpClient()),
            GifCache(cache), GifMediaCache(media), pageSize = 2, freshTtlMs = -1,
        )
        server.enqueue(MockResponse().setResponseCode(500))
        assertEquals(2, r2.search("dog", 0).size)
    }
}

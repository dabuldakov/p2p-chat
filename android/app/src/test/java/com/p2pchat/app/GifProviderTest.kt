package com.p2pchat.app

import com.p2pchat.app.gif.Gif
import com.p2pchat.app.gif.GifRef
import org.junit.Assert.assertEquals
import org.junit.Test

class GifProviderTest {

    private class FakeProvider : GifProvider {
        var sent: Gif? = null
        override fun myGifRefs(limit: Int) = listOf(GifRef("my:$limit"))
        override fun groupGifRefs(limit: Int) = listOf(GifRef("group:$limit"))
        override suspend fun searchGifs(query: String, page: Int) =
            listOf(Gif(providerId = "$query:$page", url = "https://example.test/gif"))
        override fun cachedGif(providerId: String) = if (providerId == "cached") {
            Gif(providerId, url = "https://example.test/cached")
        } else null
        override suspend fun resolveGif(providerId: String) = cachedGif(providerId)
        override fun sendGif(gif: Gif) { sent = gif }
    }

    @Test
    fun interfaceDefaultsUseSixtyItems() {
        val provider: GifProvider = FakeProvider()

        assertEquals("my:60", provider.myGifRefs().single().providerId)
        assertEquals("group:60", provider.groupGifRefs().single().providerId)
    }

    @Test
    fun providerExposesCachedGifAndSend() {
        val fake = FakeProvider()
        val provider: GifProvider = fake
        val cached = Gif(providerId = "cached", url = "https://example.test/cached")

        assertEquals(cached, provider.cachedGif("cached"))
        provider.sendGif(cached)
        assertEquals(cached, fake.sent)
    }
}

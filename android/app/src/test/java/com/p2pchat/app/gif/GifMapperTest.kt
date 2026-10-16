package com.p2pchat.app.gif

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class GifMapperTest {

    private fun media(url: String, w: Int = 220, h: Int = 230) =
        KlipyMediaDto(url = url, width = w, height = h, size = 1)

    private fun dto(
        file: Map<String, Map<String, KlipyMediaDto>>,
        slug: String? = "slug-1",
        id: Long? = 1,
    ) = KlipyGifDto(id = id, slug = slug, title = "t", file = file)

    @Test
    fun prefers_webp_and_static_preview() {
        val gif = dto(
            mapOf(
                "hd" to mapOf("webp" to media("hd.webp"), "gif" to media("hd.gif")),
                "sm" to mapOf("jpg" to media("sm.jpg")),
            ),
        ).toGif()!!
        assertEquals("hd.webp", gif.url)
        assertEquals("sm.jpg", gif.previewUrl)
        assertEquals("slug-1", gif.providerId)
    }

    @Test
    fun falls_back_to_gif_when_no_webp() {
        val gif = dto(mapOf("md" to mapOf("gif" to media("md.gif")))).toGif()!!
        assertEquals("md.gif", gif.url)
    }

    @Test
    fun maps_mp4_hd_md_sm() {
        val gif = dto(
            mapOf(
                "hd" to mapOf("webp" to media("w.webp")),
                "md" to mapOf("mp4" to media("md.mp4")),
            ),
        ).toGif()!!
        assertEquals("md.mp4", gif.mp4Url)
        assertNull(gif.previewUrl?.takeIf { it == "md.mp4" })
    }

    @Test
    fun uses_id_when_slug_missing() {
        val gif = dto(mapOf("hd" to mapOf("webp" to media("w.webp"))), slug = null, id = 42).toGif()!!
        assertEquals("42", gif.providerId)
    }

    @Test
    fun null_without_url_or_identifier() {
        assertNull(dto(emptyMap()).toGif())
        assertNull(dto(mapOf("hd" to mapOf("webp" to media("w.webp"))), slug = null, id = null).toGif())
    }
}

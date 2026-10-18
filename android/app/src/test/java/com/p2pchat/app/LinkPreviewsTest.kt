package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LinkPreviewsTest {

    @Test
    fun parses_open_graph() {
        val html = """
            <html><head>
            <meta property="og:title" content="Заголовок &amp; ещё">
            <meta property="og:image" content="/img/cover.png">
            <meta property="og:site_name" content="Example">
            <meta property="og:description" content="Описание">
            <title>fallback</title>
            </head></html>
        """.trimIndent()
        val p = LinkPreviews.parse(html, "https://example.com/page")!!
        assertEquals("Заголовок & ещё", p.title)
        assertEquals("https://example.com/img/cover.png", p.image)
        assertEquals("Example", p.site)
        assertEquals("Описание", p.description)
    }

    @Test
    fun falls_back_to_title_tag() {
        val html = "<html><head><title>Просто тайтл</title></head><body></body></html>"
        val p = LinkPreviews.parse(html, "https://site.org/a")!!
        assertEquals("Просто тайтл", p.title)
        assertNull(p.image)
        assertEquals("site.org", p.site)
    }

    @Test
    fun accepts_name_attribute_order() {
        val html = """<meta content="T" name="og:title">"""
        assertEquals("T", LinkPreviews.parse(html, "https://x.ru/")!!.title)
    }

    @Test
    fun null_without_title() {
        assertNull(LinkPreviews.parse("<html><body>нет мета</body></html>", "https://x.ru/"))
    }
}

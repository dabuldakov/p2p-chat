package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LinksTest {

    @Test
    fun finds_urls_with_ranges() {
        val text = "смотри https://example.com/a и http://test.ru"
        val spans = urlSpans(text)
        assertEquals(2, spans.size)
        assertEquals("https://example.com/a", spans[0].url)
        assertEquals("http://test.ru", spans[1].url)
        assertEquals(
            "https://example.com/a",
            text.substring(spans[0].start, spans[0].end),
        )
    }

    @Test
    fun no_urls() {
        assertTrue(urlSpans("просто текст без ссылок").isEmpty())
    }

    @Test
    fun only_http_scheme() {
        assertTrue(urlSpans("ftp://example.com и mailto:a@b.c").isEmpty())
        assertEquals(1, urlSpans("http://ok.dev").size)
    }
}

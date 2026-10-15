package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ChangelogParserTest {

    @Test
    fun groups_and_items() {
        val body = """
            Версия: 1.2.3
            ## Новое
            - проверка обновлений
            - бейдж
            ## Исправления
            - краш при старте
            Прочие строки игнорируются
        """.trimIndent()

        val groups = parseChangelog(body)
        assertEquals(2, groups.size)
        assertEquals("Новое", groups[0].title)
        assertEquals(listOf("проверка обновлений", "бейдж"), groups[0].items)
        assertEquals("Исправления", groups[1].title)
        assertEquals(listOf("краш при старте"), groups[1].items)
    }

    @Test
    fun blank_and_empty() {
        assertTrue(parseChangelog(null).isEmpty())
        assertTrue(parseChangelog("").isEmpty())
        assertTrue(parseChangelog("просто текст\nбез заголовков и маркеров").isEmpty())
    }

    @Test
    fun empty_groups_are_skipped() {
        val body = "## Пустая\n\n## С пунктом\n- один"
        val groups = parseChangelog(body)
        assertEquals(1, groups.size)
        assertEquals("С пунктом", groups[0].title)
    }
}
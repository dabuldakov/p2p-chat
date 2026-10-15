package com.p2pchat.app

/** Группа изменений «что нового»: заголовок раздела и его пункты. */
data class ChangelogGroup(
    val title: String,
    val items: List<String>,
)

/**
 * Разбирает Markdown-тело релиза в группы. Заголовок группы — строка,
 * начинающаяся с '#', пункты — строки-маркеры ('-' или '*'). Прочие строки
 * игнорируются; пустые группы не возвращаются.
 */
internal fun parseChangelog(body: String?): List<ChangelogGroup> {
    if (body.isNullOrBlank()) return emptyList()

    val groups = mutableListOf<ChangelogGroup>()
    var title: String? = null
    val items = mutableListOf<String>()

    fun flush() {
        val current = title
        if (current != null && items.isNotEmpty()) {
            groups += ChangelogGroup(current, items.toList())
        }
        items.clear()
    }

    for (raw in body.lineSequence()) {
        val line = raw.trim()
        when {
            line.isEmpty() -> Unit
            line.startsWith("#") -> {
                flush()
                title = line.trimStart('#').trim()
            }
            line.startsWith("- ") || line.startsWith("* ") -> {
                line.substring(2).trim().takeIf { it.isNotEmpty() }?.let(items::add)
            }
            else -> Unit
        }
    }
    flush()
    return groups
}
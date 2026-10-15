package com.p2pchat.app

/** Найденная в тексте ссылка: диапазон [start, end) и сам url. */
data class UrlSpan(val start: Int, val end: Int, val url: String)

private val URL_REGEX = Regex("""https?://\S+""")

/** Все ссылки http(s):// в тексте. Чистая функция — легко тестируется. */
fun urlSpans(text: String): List<UrlSpan> =
    URL_REGEX.findAll(text)
        .map { UrlSpan(it.range.first, it.range.last + 1, it.value) }
        .toList()
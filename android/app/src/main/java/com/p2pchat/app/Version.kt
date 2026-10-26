package com.p2pchat.app

/**
 * Сравнение версий вида «1.2.0». Нечисловые части (например, суффикс «-beta»)
 * трактуются как 0 — этого достаточно для релизных тегов приложения.
 */
internal fun isNewerVersion(latest: String, current: String): Boolean {
    val latestParts = latest.split('.')
    val currentParts = current.split('.')
    for (i in 0 until maxOf(latestParts.size, currentParts.size)) {
        val l = latestParts.getOrNull(i)?.toIntOrNull() ?: 0
        val c = currentParts.getOrNull(i)?.toIntOrNull() ?: 0
        if (l != c) return l > c
    }
    return false
}

package com.p2pchat.app.gif

import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File

/**
 * Постоянный кэш первых страниц поиска/трендов.
 *
 * Гифки неизменны, поэтому храним содержимое на устройстве (JSON в filesDir):
 * повторный вход и перезапуск показываются мгновенно, в сеть идём только за
 * новыми ключами. Свежесть ограничена TTL — по истечении обновляем по требованию.
 * Инвалидация — [clear].
 */
class GifCache(
    private val file: File,
    private val now: () -> Long = System::currentTimeMillis,
) {
    private val json = Json { ignoreUnknownKeys = true }
    private val lock = Any()
    private var loaded = false
    private val entries = LinkedHashMap<String, Entry>()

    @Serializable
    private data class Entry(val at: Long, val items: List<Gif>)

    /** Свежая запись (моложе [ttlMs]) — сеть не нужна. */
    fun getFresh(key: String, ttlMs: Long): List<Gif>? = synchronized(lock) {
        ensureLoaded()
        val e = entries[key] ?: return null
        if (now() - e.at > ttlMs) null else e.items
    }

    /** Запись независимо от возраста — показываем, если сети нет. */
    fun getStale(key: String): List<Gif>? = synchronized(lock) {
        ensureLoaded()
        entries[key]?.items
    }

    fun put(key: String, items: List<Gif>) = synchronized(lock) {
        ensureLoaded()
        entries.remove(key)
        entries[key] = Entry(now(), items.take(MAX_PAGE_ITEMS))
        while (entries.size > MAX_ENTRIES) {
            val oldest = entries.keys.firstOrNull() ?: break
            entries.remove(oldest)
        }
        persist()
    }

    fun clear() = synchronized(lock) {
        ensureLoaded()
        entries.clear()
        persist()
    }

    private fun ensureLoaded() {
        if (loaded) return
        loaded = true
        runCatching {
            if (file.exists()) {
                val map: LinkedHashMap<String, Entry> =
                    json.decodeFromString(file.readText())
                entries.putAll(map)
            }
        }
    }

    private fun persist() {
        runCatching {
            file.parentFile?.mkdirs()
            file.writeText(json.encodeToString(entries))
        }
    }

    companion object {
        private const val MAX_ENTRIES = 100
        private const val MAX_PAGE_ITEMS = 60

        fun trendingKey(): String = "trending"
        fun searchKey(query: String): String = "search:${query.trim().lowercase()}"
    }
}

package com.p2pchat.app.gif

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.io.File

/**
 * Постоянный кэш разрезолвленных гифок: `providerId -> Gif` (url/mp4/preview/размеры).
 *
 * Заполняется при выборе в пикере, при отправке и при получении сообщения. Однажды
 * увидев гифку, рендерим её офлайн, не дёргая провайдера повторно.
 */
class GifMediaCache(private val file: File) {
    private val json = Json { ignoreUnknownKeys = true }
    private val lock = Any()
    private var loaded = false
    private val entries = LinkedHashMap<String, Gif>()

    fun get(providerId: String): Gif? = synchronized(lock) {
        ensureLoaded()
        entries[providerId]
    }

    fun put(providerId: String, gif: Gif) = synchronized(lock) {
        ensureLoaded()
        entries.remove(providerId)
        entries[providerId] = gif
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

    fun sizeBytes(): Long = file.takeIf { it.isFile }?.length() ?: 0L

    private fun ensureLoaded() {
        if (loaded) return
        loaded = true
        runCatching {
            if (file.exists()) {
                val map: LinkedHashMap<String, Gif> = json.decodeFromString(file.readText())
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
        private const val MAX_ENTRIES = 500
    }
}

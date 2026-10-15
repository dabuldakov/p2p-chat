package com.p2pchat.app

import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONArray
import org.json.JSONObject

/** Доступное обновление: версия, ссылка на скачивание и «что нового» по группам. */
data class AppUpdate(
    val version: String,
    val downloadUrl: String,
    val changelog: List<ChangelogGroup> = emptyList(),
)

/**
 * Проверка новых релизов через GitHub.
 *
 * Берёт список релизов, отбирает все, что новее установленной, и объединяет их
 * «что нового» (от старой версии к новой). Так, если между установленной и
 * последней версией было несколько релизов, пользователь увидит изменения всех.
 */
object UpdateChecker {
    private val client = OkHttpClient()
    private const val LIST_API =
        "https://api.github.com/repos/dabuldakov/p2p-chat/releases?per_page=30"

    fun check(installed: String): AppUpdate? {
        val releases = fetchReleases() ?: return null

        // все релизы новее установленной, с их версией
        val newer = releases
            .mapNotNull { o -> versionOf(o)?.let { it to o } }
            .filter { (v, _) -> isNewerVersion(v, installed) }
        if (newer.isEmpty()) return null

        val sorted = newer.sortedBy { (v, _) -> versionKey(v) }
        val latest = sorted.last()
        val latestVersion = latest.first
        val apkUrl = pickApk(latest.second, latestVersion) ?: latest.second.optString("html_url")
        if (apkUrl.isBlank()) return null

        // объединяем changelog всех новых релизов, группируя по заголовку
        val merged = LinkedHashMap<String, MutableList<String>>()
        for ((_, o) in sorted) {
            for (group in parseChangelog(o.optString("body"))) {
                merged.getOrPut(group.title) { mutableListOf() }.addAll(group.items)
            }
        }
        val changelog = merged.map { ChangelogGroup(it.key, it.value) }

        return AppUpdate(latestVersion, apkUrl, changelog)
    }

    private fun fetchReleases(): JSONArray? {
        val request = Request.Builder()
            .url(LIST_API)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "p2pchat-android")
            .build()
        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) return null
                val raw = resp.body?.string() ?: return null
                JSONArray(raw)
            }
        } catch (e: Exception) {
            null
        }
    }

    private fun versionOf(json: JSONObject): String? {
        val tag = json.optString("tag_name").removePrefix("v").trim()
        if (tag.matches(Regex("""\d+\.\d+\.\d+"""))) return tag
        val text = json.optString("name") + " " + json.optString("body")
        return Regex("""(\d+)\.(\d+)\.(\d+)""").find(text)?.value
    }

    private fun versionKey(v: String): Int {
        val p = v.split('.')
        return (p.getOrNull(0)?.toIntOrNull() ?: 0) * 10_000 +
            (p.getOrNull(1)?.toIntOrNull() ?: 0) * 100 +
            (p.getOrNull(2)?.toIntOrNull() ?: 0)
    }

    private fun pickApk(json: JSONObject, version: String): String? {
        val assets = json.optJSONArray("assets") ?: return null
        val wanted = "p2p-chat-$version.apk"
        var fallback: String? = null
        for (i in 0 until assets.length()) {
            val a = assets.getJSONObject(i)
            val name = a.optString("name")
            if (!name.startsWith("p2p-chat-") || !name.endsWith(".apk") || name.contains("debug")) continue
            val url = a.optString("browser_download_url")
            if (name == wanted) return url
            if (fallback == null) fallback = url
        }
        return fallback
    }
}
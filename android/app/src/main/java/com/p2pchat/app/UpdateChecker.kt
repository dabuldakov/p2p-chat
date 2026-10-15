package com.p2pchat.app

import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject

/** Информация о доступном обновлении. */
data class UpdateInfo(
    val version: String,
    val changelog: String,
    val apkUrl: String,
)

/**
 * Проверка обновлений через GitHub Releases.
 * Сравнивает установленную версию с последним релизом и, если он новее,
 * возвращает версию, список изменений и прямую ссылку на APK.
 */
object UpdateChecker {
    private val client = OkHttpClient()
    private const val API =
        "https://api.github.com/repos/dabuldakov/p2p-chat/releases/latest"

    fun check(installed: String): UpdateInfo? {
        val request = Request.Builder()
            .url(API)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", "p2pchat-android")
            .build()
        return try {
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) return null
                val raw = resp.body?.string() ?: return null
                parse(raw, installed)
            }
        } catch (e: Exception) {
            null
        }
    }

    private fun parse(raw: String, installed: String): UpdateInfo? {
        val json = JSONObject(raw)
        val body = json.optString("body")
        val text = json.optString("name") + " " + body
        val version = Regex("""(\d+)\.(\d+)\.(\d+)""").find(text)?.value ?: return null
        if (compareVersions(version, installed) <= 0) return null
        val apkUrl = pickApk(json, version) ?: return null
        return UpdateInfo(version, extractChangelog(body), apkUrl)
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

    private fun extractChangelog(body: String): String {
        val idx = body.indexOf("Что нового")
        return if (idx >= 0) body.substring(idx).trim() else body.trim()
    }

    /** Сравнение semver: >0 если a новее b. */
    fun compareVersions(a: String, b: String): Int {
        val pa = a.split(".").map { it.toIntOrNull() ?: 0 }
        val pb = b.split(".").map { it.toIntOrNull() ?: 0 }
        for (i in 0..2) {
            val d = (pa.getOrNull(i) ?: 0) - (pb.getOrNull(i) ?: 0)
            if (d != 0) return d
        }
        return 0
    }
}
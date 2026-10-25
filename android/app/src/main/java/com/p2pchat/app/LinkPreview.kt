package com.p2pchat.app

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import coil.compose.AsyncImage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.net.URLEncoder
import java.nio.charset.StandardCharsets
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.util.concurrent.TimeUnit

/** Данные превью ссылки (Open Graph). */
@Serializable
data class LinkPreview(
    val url: String,
    val title: String,
    val image: String? = null,
    val site: String? = null,
    val description: String? = null,
)

private fun normalizedHost(raw: String): String? =
    raw.toHttpUrlOrNull()?.host?.removePrefix("www.")?.removePrefix("m.")

internal fun youtubeId(raw: String): String? {
    val u = raw.toHttpUrlOrNull() ?: return null
    val host = normalizedHost(raw) ?: return null
    val id = when (host) {
        "youtu.be" -> u.pathSegments.firstOrNull()
        "youtube.com", "music.youtube.com" -> when (u.pathSegments.firstOrNull()) {
            "watch" -> u.queryParameter("v")
            "shorts", "embed", "live", "v" -> u.pathSegments.getOrNull(1)
            else -> null
        }
        else -> null
    }
    return id?.takeIf { it.matches(Regex("[A-Za-z0-9_-]{6,}")) }
}

internal fun rutubeId(raw: String): String? {
    val u = raw.toHttpUrlOrNull() ?: return null
    if (normalizedHost(raw) != "rutube.ru") return null
    val p = u.pathSegments
    val id = when (p.firstOrNull()) {
        "video", "shorts" -> p.getOrNull(1)
        "play" -> if (p.getOrNull(1) == "embed") p.getOrNull(2) else null
        else -> null
    }
    return id?.substringBefore('.')?.takeIf { it.matches(Regex("[A-Za-z0-9_-]{6,}")) }
}

internal fun instagramCode(raw: String): String? {
    val u = raw.toHttpUrlOrNull() ?: return null
    if (normalizedHost(raw) !in setOf("instagram.com", "instagr.am")) return null
    if (u.pathSegments.firstOrNull() !in setOf("p", "reel", "tv")) return null
    return u.pathSegments.getOrNull(1)?.takeIf { it.matches(Regex("[A-Za-z0-9_-]{4,}")) }
}

/**
 * Загрузка и кэш превью ссылок (Open Graph). Тянем HTML с клиента, парсим
 * `og:title`/`og:image`/`<title>`, кэшируем в памяти и на диске (по URL).
 */
object LinkPreviews {
    private val client = OkHttpClient.Builder()
        .connectTimeout(10, TimeUnit.SECONDS)
        .readTimeout(10, TimeUnit.SECONDS)
        .build()
    private val json = Json { ignoreUnknownKeys = true }
    private const val MAX_HTML = 512 * 1024
    private const val BROWSER_UA =
        "Mozilla/5.0 (Linux; Android 13) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Mobile Safari/537.36"
    private const val FACEBOOK_UA = "facebookexternalhit/1.1"

    private val mem = HashMap<String, LinkPreview?>()
    private var file: File? = null
    private val disk = LinkedHashMap<String, LinkPreview>()
    private var loaded = false

    private fun init(ctx: Context) {
        if (loaded) return
        loaded = true
        file = File(ctx.applicationContext.filesDir, "link_previews.json")
        runCatching {
            file?.takeIf { it.exists() }?.let {
                val map: LinkedHashMap<String, LinkPreview> = json.decodeFromString(it.readText())
                disk.putAll(map)
            }
        }
    }

    fun cached(ctx: Context, url: String): LinkPreview? {
        init(ctx)
        return if (mem.containsKey(url)) mem[url] else disk[url]
    }

    fun resolved(ctx: Context, url: String): Boolean {
        init(ctx)
        return mem.containsKey(url) || disk.containsKey(url)
    }

    suspend fun fetch(ctx: Context, url: String): LinkPreview? {
        init(ctx)
        if (mem.containsKey(url)) return mem[url]
        val preview = withContext(Dispatchers.IO) {
            runCatching { resolve(url) }.getOrNull()
        }
        mem[url] = preview
        if (preview != null) {
            disk.remove(url)
            disk[url] = preview
            while (disk.size > 200) disk.remove(disk.keys.first())
            save()
        }
        return preview
    }

    private fun resolve(url: String): LinkPreview? {
        val host = normalizedHost(url) ?: return null

        youtubeId(url)?.let { id ->
            oembed("https://www.youtube.com/oembed?url=${enc(url)}&format=json")?.let {
                return it.copy(
                    url = url,
                    site = "YouTube",
                    image = it.image ?: "https://img.youtube.com/vi/$id/hqdefault.jpg",
                )
            }
            return fetchHtml(url, BROWSER_UA)?.let { parse(it, url)?.copy(site = "YouTube") }
        }

        rutubeId(url)?.let {
            oembed("https://rutube.ru/api/oembed/?url=${enc(url)}&format=json")?.let { p ->
                return p.copy(url = url, site = "RuTube")
            }
            return fetchHtml(url, BROWSER_UA)?.let { parse(it, url)?.copy(site = "RuTube") }
        }

        instagramCode(url)?.let { code ->
            return instagramPreview(url, code)
                ?: fetchHtml(url, BROWSER_UA)?.let { parse(it, url)?.copy(site = "Instagram") }
        }

        if (host.endsWith("facebook.com") || host in setOf("fb.com", "fb.watch", "fb.me")) {
            return fetchHtml(url, FACEBOOK_UA)?.let { parse(it, url)?.copy(site = "Facebook") }
        }
        if (host.endsWith("vk.com") || host.endsWith("vk.ru") || host == "vk.cc") {
            return fetchHtml(url, BROWSER_UA)?.let { parse(it, url)?.copy(site = "VK") }
        }
        return fetchHtml(url, BROWSER_UA)?.let { parse(it, url) }
    }

    private fun fetchHtml(url: String, ua: String): String? = runCatching {
        val req = Request.Builder()
            .url(url)
            .header("User-Agent", ua)
            .header("Accept-Language", "ru-RU,ru;q=0.9,en;q=0.8")
            .build()
        client.newCall(req).execute().use { resp ->
            if (!resp.isSuccessful) return@use null
            val src = resp.body?.source() ?: return@use null
            src.request(MAX_HTML.toLong())
            src.buffer.readUtf8()
        }
    }.getOrNull()

    private fun oembed(endpoint: String): LinkPreview? = runCatching {
        val body = fetchHtml(endpoint, BROWSER_UA) ?: return null
        val obj = json.parseToJsonElement(body).jsonObject
        val title = obj["title"]?.jsonPrimitive?.contentOrNull ?: return null
        val image = obj["thumbnail_url"]?.jsonPrimitive?.contentOrNull
        val author = obj["author_name"]?.jsonPrimitive?.contentOrNull
        LinkPreview(
            url = "",
            title = unescape(title),
            image = image,
            description = author,
        )
    }.getOrNull()

    private fun instagramPreview(url: String, code: String): LinkPreview? {
        val html = fetchHtml("https://www.instagram.com/p/$code/embed/captioned/", BROWSER_UA)
        if (html != null) {
            val title = meta(html, "og:title")
                ?: Regex("\"title\":\"([^\"]+)\"").find(html)?.groupValues?.get(1)
            val rawImage = Regex("\"(?:thumbnail_src|display_url)\":\"(https:[^\"]+)\"")
                .find(html)?.groupValues?.get(1)?.replace("\\/", "/")
            if (title != null || rawImage != null) {
                return LinkPreview(
                    url = url,
                    title = unescape(title ?: "Instagram"),
                    image = rawImage?.let(::unescape),
                    site = "Instagram",
                )
            }
        }
        return LinkPreview(
            url = url,
            title = "Instagram",
            image = "https://www.instagram.com/p/$code/media/?size=l",
            site = "Instagram",
        )
    }

    private fun enc(s: String): String =
        URLEncoder.encode(s, StandardCharsets.UTF_8.toString())

    private fun save() {
        runCatching { file?.writeText(json.encodeToString(LinkedHashMap(disk))) }
    }

    /** Разобрать HTML: Open Graph с фолбэком на `<title>`. */
    fun parse(html: String, baseUrl: String): LinkPreview? {
        val title = meta(html, "og:title") ?: titleTag(html) ?: return null
        val imageRaw = meta(html, "og:image") ?: meta(html, "twitter:image")
        val site = meta(html, "og:site_name") ?: baseUrl.toHttpUrlOrNull()?.host
        val desc = meta(html, "og:description") ?: meta(html, "description")
        return LinkPreview(
            url = baseUrl,
            title = unescape(title),
            image = imageRaw?.let { absolute(baseUrl, it) },
            site = site,
            description = desc?.let { unescape(it) },
        )
    }

    private fun meta(html: String, key: String): String? {
        val tag = Regex(
            "<meta[^>]+(?:property|name)=[\"']${Regex.escape(key)}[\"'][^>]*>",
            RegexOption.IGNORE_CASE,
        ).find(html)?.value ?: return null
        return Regex("content=[\"']([^\"']*)[\"']", RegexOption.IGNORE_CASE)
            .find(tag)?.groupValues?.get(1)?.takeIf { it.isNotBlank() }
    }

    private fun titleTag(html: String): String? =
        Regex("<title[^>]*>(.*?)</title>", setOf(RegexOption.IGNORE_CASE, RegexOption.DOT_MATCHES_ALL))
            .find(html)?.groupValues?.get(1)?.trim()?.takeIf { it.isNotBlank() }

    private fun absolute(base: String, url: String): String =
        base.toHttpUrlOrNull()?.resolve(url)?.toString() ?: url

    private fun unescape(s: String): String = s
        .replace("&amp;", "&").replace("&quot;", "\"")
        .replace("&#39;", "'").replace("&apos;", "'")
        .replace("&lt;", "<").replace("&gt;", ">").trim()
}

/** Карточка-превью ссылки. Если превью не удалось — обычный текст ссылки. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun LinkPreviewCard(url: String, onLongPress: () -> Unit = {}, onFallback: @Composable () -> Unit) {
    val ctx = LocalContext.current
    var preview by remember(url) { mutableStateOf(LinkPreviews.cached(ctx, url)) }

    LaunchedEffect(url) {
        preview = if (LinkPreviews.resolved(ctx, url)) LinkPreviews.cached(ctx, url)
        else LinkPreviews.fetch(ctx, url)
    }

    val p = preview
    if (p == null) {
        onFallback()
        return
    }

    val open = {
        runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
        Unit
    }
    Surface(
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(12.dp),
        modifier = Modifier
            .widthIn(max = 260.dp)
            .clip(RoundedCornerShape(12.dp))
            .combinedClickable(onClick = { open() }, onLongClick = onLongPress),
    ) {
        Column {
            p.image?.let { img ->
                AsyncImage(
                    model = img,
                    contentDescription = p.title,
                    contentScale = ContentScale.Crop,
                    modifier = Modifier.fillMaxWidth().height(140.dp),
                )
            }
            Column(Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    p.title,
                    style = MaterialTheme.typography.bodyMedium,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 2,
                )
                val host = p.site ?: url.toHttpUrlOrNull()?.host
                if (!host.isNullOrBlank()) {
                    Text(
                        host,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

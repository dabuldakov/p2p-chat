package com.p2pchat.app.gif

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.json.Json
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.IOException
import java.util.concurrent.TimeUnit

/**
 * Клиент публичного GIF-API KLIPY.
 *
 * Запросы идут **напрямую с устройства** (требование провайдера); ключ встроен в
 * сборку. Отдельный OkHttp без наших интерцепторов/токенов — это внешний хост.
 */
class KlipyApi(
    private val key: String,
    baseUrl: String = DEFAULT_BASE,
    private val client: OkHttpClient = defaultClient(),
) {
    private val base: HttpUrl = baseUrl.toHttpUrl()
    private val json = Json { ignoreUnknownKeys = true; isLenient = true }

    /** API доступно только если в сборку встроен ключ. */
    val enabled: Boolean get() = key.isNotBlank()

    suspend fun search(
        query: String,
        page: Int,
        perPage: Int,
        contentFilter: String = CONTENT_FILTER,
    ): List<KlipyGifDto> =
        fetchPage("gifs/search", page, perPage, contentFilter) { it.addQueryParameter("q", query) }

    suspend fun trending(
        page: Int,
        perPage: Int,
        contentFilter: String = CONTENT_FILTER,
    ): List<KlipyGifDto> =
        fetchPage("gifs/trending", page, perPage, contentFilter) {}

    /** Одна гифка по id/slug (нужна для резолва вложения на принимающей стороне). */
    suspend fun byId(id: String): KlipyGifDto? = withContext(Dispatchers.IO) {
        val url = urlBuilder()
            .addPathSegment("gifs")
            .addPathSegment(id)
            .build()
        val body = get(url) ?: return@withContext null
        runCatching { json.decodeFromString<KlipyGifResponse>(body).data }.getOrNull()
    }

    private suspend fun fetchPage(
        path: String,
        page: Int,
        perPage: Int,
        contentFilter: String,
        extra: (HttpUrl.Builder) -> Unit,
    ): List<KlipyGifDto> = withContext(Dispatchers.IO) {
        val b = urlBuilder().addPathSegment("gifs").addPathSegment(path.substringAfterLast('/'))
        b.addQueryParameter("page", page.toString())
        b.addQueryParameter("per_page", perPage.toString())
        b.addQueryParameter("content_filter", contentFilter)
        extra(b)
        val body = get(b.build()) ?: return@withContext emptyList()
        runCatching { json.decodeFromString<KlipyGifsResponse>(body).items() }.getOrDefault(emptyList())
    }

    /** GET -> тело или null (404/неуспех). Сетевые сбои пробрасываются. */
    private fun get(url: HttpUrl): String? {
        client.newCall(Request.Builder().url(url).build()).execute().use { resp ->
            if (!resp.isSuccessful) {
                if (resp.code == 404) return null
                throw IOException("klipy http ${resp.code}")
            }
            return resp.body?.string()
        }
    }

    private fun urlBuilder(): HttpUrl.Builder =
        base.newBuilder().addPathSegment("api").addPathSegment("v1").addPathSegment(key)

    companion object {
        const val DEFAULT_BASE = "https://api.klipy.com"
        /** Безопасный контент. */
        const val CONTENT_FILTER = "high"

        fun defaultClient(): OkHttpClient =
            OkHttpClient.Builder()
                .connectTimeout(15, TimeUnit.SECONDS)
                .readTimeout(15, TimeUnit.SECONDS)
                .build()
    }
}

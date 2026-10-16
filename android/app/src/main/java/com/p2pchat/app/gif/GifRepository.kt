package com.p2pchat.app.gif

import kotlinx.coroutines.CancellationException

/**
 * Гифки: поиск/тренды идут напрямую в KLIPY (как требует провайдер), а также
 * резолв вложения по `providerId`.
 *
 * Первая страница кэшируется на устройстве ([GifCache]): пока запись свежая —
 * сеть не трогаем; иначе обновляем при следующем открытии; при отсутствии сети
 * отдаём старый кэш. Разрезолвленные гифки кладём в [GifMediaCache] — их потом
 * показываем офлайн (и в переписке, и во вкладках «Мои»/«Эта группа»).
 */
class GifRepository(
    private val api: KlipyApi,
    private val cache: GifCache,
    private val mediaCache: GifMediaCache,
    private val pageSize: Int = 30,
    private val freshTtlMs: Long = FRESH_TTL_MS,
) {
    val enabled: Boolean get() = api.enabled

    suspend fun search(query: String, page: Int): List<Gif> {
        val key = if (query.isBlank()) GifCache.trendingKey() else GifCache.searchKey(query)
        return cached(page, key) {
            val dtos = if (query.isBlank()) {
                api.trending(page + 1, pageSize)
            } else {
                api.search(query, page + 1, pageSize)
            }
            dtos.mapNotNull { it.toGif() }.onEach { mediaCache.put(it.providerId, it) }
        }
    }

    /** Резолв по id: сначала кэш, затем провайдер. */
    suspend fun resolve(providerId: String): Gif? {
        mediaCache.get(providerId)?.let { return it }
        val gif = try {
            api.byId(providerId)?.toGif()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            null
        } ?: return null
        mediaCache.put(providerId, gif)
        return gif
    }

    /** Синхронно, только из кэша (для быстрого первичного рендера). */
    fun cached(providerId: String): Gif? = mediaCache.get(providerId)

    private suspend fun cached(page: Int, key: String, fetch: suspend () -> List<Gif>): List<Gif> {
        if (page == 0) {
            cache.getFresh(key, freshTtlMs)?.let { return it }
        }
        val items = try {
            fetch()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            emptyList()
        }
        if (page == 0) {
            if (items.isNotEmpty()) {
                cache.put(key, items)
                return items
            }
            return cache.getStale(key) ?: emptyList()
        }
        return items
    }

    companion object {
        const val FRESH_TTL_MS = 30 * 60 * 1000L
    }
}

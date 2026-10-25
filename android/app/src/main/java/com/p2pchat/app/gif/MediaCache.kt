package com.p2pchat.app.gif

import android.content.Context
import androidx.media3.database.StandaloneDatabaseProvider
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.cache.CacheDataSource
import androidx.media3.datasource.cache.LeastRecentlyUsedCacheEvictor
import androidx.media3.datasource.cache.SimpleCache
import androidx.media3.datasource.okhttp.OkHttpDataSource
import okhttp3.OkHttpClient
import java.io.File
import java.util.concurrent.TimeUnit

/**
 * Единый дисковый кэш медиа для ExoPlayer (mp4-гифки).
 *
 * Один общий [SimpleCache] на процесс: скачанные mp4 переиспользуются во всех
 * сообщениях и сессиях, офлайн-воспроизведение и отсутствие лишних загрузок.
 * Источник — OkHttp (переиспользование соединений). Кэш ограничен по размеру LRU.
 */
object Media3Cache {
    private const val MAX_BYTES = 256L * 1024 * 1024

    @Volatile
    private var factory: DataSource.Factory? = null
    @Volatile
    private var cacheRef: SimpleCache? = null

    fun install(context: Context) {
        if (factory != null) return
        synchronized(this) {
            if (factory != null) return
            val cacheDir = File(context.cacheDir, "media3").apply { mkdirs() }
            val cache = SimpleCache(
                cacheDir,
                LeastRecentlyUsedCacheEvictor(MAX_BYTES),
                StandaloneDatabaseProvider(context),
            )
            cacheRef = cache
            val upstream = OkHttpDataSource.Factory(
                OkHttpClient.Builder()
                    .connectTimeout(15, TimeUnit.SECONDS)
                    .readTimeout(30, TimeUnit.SECONDS)
                    .build(),
            )
            factory = CacheDataSource.Factory()
                .setCache(cache)
                .setUpstreamDataSourceFactory(upstream)
                .setFlags(CacheDataSource.FLAG_IGNORE_CACHE_ON_ERROR)
        }
    }

    fun dataSourceFactory(): DataSource.Factory =
        factory ?: error("Media3Cache.install() не вызван")

    fun sizeBytes(context: Context): Long {
        val dir = File(context.cacheDir, "media3")
        return dir.walkTopDown().filter { it.isFile }.sumOf { it.length() }
    }

    @OptIn(coil.annotation.ExperimentalCoilApi::class)
    fun clear(context: Context) {
        synchronized(this) {
            runCatching { cacheRef?.release() }
            cacheRef = null
            factory = null
            File(context.cacheDir, "media3").deleteRecursively()
            install(context)
        }
    }
}

/**
 * Настройка Coil: поддержка анимированных webp/gif + общий дисковый кэш.
 */
object CoilSetup {
    @Volatile
    private var installed = false

    fun clear(context: Context) {
        val loader = coil.Coil.imageLoader(context)
        runCatching { loader.diskCache?.clear() }
        runCatching { loader.memoryCache?.clear() }
    }

    fun install(context: Context) {
        if (installed) return
        installed = true
        val loader = coil.ImageLoader.Builder(context)
            .components { add(coil.decode.GifDecoder.Factory()) }
            .crossfade(true)
            .build()
        coil.Coil.setImageLoader(loader)
    }
}

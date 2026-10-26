package com.p2pchat.app

import com.p2pchat.app.gif.Gif
import com.p2pchat.app.gif.GifRef

/** Источник GIF для UI: поиск, кэш, отправка. */
interface GifProvider {
    fun myGifRefs(limit: Int = 60): List<GifRef>
    fun groupGifRefs(limit: Int = 60): List<GifRef>
    suspend fun searchGifs(query: String, page: Int): List<Gif>
    fun cachedGif(providerId: String): Gif?
    suspend fun resolveGif(providerId: String): Gif?
    fun sendGif(gif: Gif)
}

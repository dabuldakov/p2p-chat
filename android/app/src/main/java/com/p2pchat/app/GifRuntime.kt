package com.p2pchat.app

import com.p2pchat.app.gif.Gif
import com.p2pchat.app.gif.GifRef
import com.p2pchat.app.gif.GifRepository
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiAttachment
import uniffi.p2pchat_ffi.FfiOutgoing

/** GIF-поиск, история и отправка; отделён от общего рантайма чата. */
internal class GifRuntime(
    private val core: ChatCore,
    private val gifs: GifRepository,
    private val currentGroup: () -> ByteArray?,
    private val post: ((ChatCore) -> FfiOutgoing?) -> Unit,
) {
    val enabled: Boolean get() = gifs.enabled

    fun send(gif: Gif) {
        val group = currentGroup() ?: return
        post {
            it.postGif(
                group,
                FfiAttachment(
                    kind = "klipy",
                    id = gif.providerId,
                    width = (gif.width ?: 0).toUInt(),
                    height = (gif.height ?: 0).toUInt(),
                    url = "",
                    mime = "",
                    size = 0uL,
                    durationMs = 0uL,
                    fileName = "",
                    thumbnail = ByteArray(0),
                ),
            )
        }
    }

    suspend fun search(query: String, page: Int): List<Gif> = gifs.search(query, page)
    suspend fun resolve(providerId: String): Gif? = gifs.resolve(providerId)
    fun cached(providerId: String): Gif? = gifs.cached(providerId)

    fun recentRefs(limit: Int): List<GifRef> =
        core.recentGifs(limit.toUInt()).map(::toGifRef)

    fun groupRefs(limit: Int): List<GifRef> {
        val group = currentGroup() ?: return emptyList()
        return core.groupGifs(group, limit.toUInt()).map(::toGifRef)
    }
}

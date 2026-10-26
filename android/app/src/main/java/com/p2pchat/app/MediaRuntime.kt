package com.p2pchat.app

import android.app.Application
import com.p2pchat.app.gif.GifRepository
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiAttachment
import uniffi.p2pchat_ffi.FfiMessage
import uniffi.p2pchat_ffi.FfiOutgoing

/** Отправка и догрузка вложений; очистка кэша. */
internal class MediaRuntime(
    private val app: Application,
    private val core: ChatCore,
    private val media: MediaTransfer,
    private val gifs: GifRepository,
    private val currentGroup: () -> ByteArray?,
    private val messages: () -> List<FfiMessage>,
    private val post: ((ChatCore) -> FfiOutgoing?) -> Unit,
) {
    private val skip = mutableSetOf<String>()

    fun ensure(group: ByteArray) {
        for (message in messages()) {
            val att = message.attachment ?: continue
            if (att.kind in setOf("image", "video", "audio", "voice", "video_note") &&
                !MediaFiles.exists(app, att.id, att.mime) &&
                att.id !in skip
            ) {
                media.ensure(group, att.id, att.mime)
            }
        }
    }

    fun retry(id: String, mime: String) {
        val group = currentGroup() ?: return
        skip.remove(id)
        media.retry(group, id, mime)
    }

    fun send(group: ByteArray, kind: String, prepared: PreparedMedia) {
        post {
            it.postAttachment(
                group,
                FfiAttachment(
                    kind = kind,
                    id = prepared.id,
                    width = prepared.width.toUInt(),
                    height = prepared.height.toUInt(),
                    url = "",
                    mime = prepared.mime,
                    size = prepared.size.toULong(),
                    durationMs = prepared.durationMs.toULong(),
                    fileName = prepared.fileName,
                    thumbnail = prepared.thumbnail ?: ByteArray(0),
                ),
            )
        }
    }

    fun stats(): List<StorageStat> = MediaStorage.stats(app, gifs)

    fun clear(id: String) {
        MediaStorage.attachmentsToSkip(id, messages().mapNotNull { it.attachment }).forEach { skip.add(it) }
        MediaStorage.clear(id, app, gifs)
    }
}

package com.p2pchat.app

import android.app.Application
import com.p2pchat.app.gif.CoilSetup
import com.p2pchat.app.gif.GifRepository
import com.p2pchat.app.gif.Media3Cache
import uniffi.p2pchat_ffi.FfiAttachment

internal object MediaStorage {

    fun stats(app: Application, gifs: GifRepository): List<StorageStat> {
        val media = MediaFiles.stats(app)
        fun pair(cat: MediaCategory) = media[cat] ?: (0L to 0)
        return listOf(
            StorageStat("photos", pair(MediaCategory.PHOTOS).first, pair(MediaCategory.PHOTOS).second),
            StorageStat("videos", pair(MediaCategory.VIDEOS).first, pair(MediaCategory.VIDEOS).second),
            StorageStat("audio", pair(MediaCategory.AUDIO).first, pair(MediaCategory.AUDIO).second),
            StorageStat("temp", pair(MediaCategory.TEMP).first, pair(MediaCategory.TEMP).second),
            StorageStat("other", pair(MediaCategory.OTHER).first, pair(MediaCategory.OTHER).second),
            StorageStat("gifs", gifs.mediaCacheSizeBytes() + Media3Cache.sizeBytes(app)),
            StorageStat("links", LinkPreviews.sizeBytes(app)),
        )
    }

    internal fun skipKinds(id: String): Set<String>? = when (id) {
        "photos" -> setOf("image")
        "videos" -> setOf("video", "video_note")
        "audio" -> setOf("audio", "voice")
        "all" -> setOf("image", "video", "video_note", "audio", "voice")
        else -> null
    }

    internal fun attachmentsToSkip(id: String, attachments: List<FfiAttachment>): List<String> {
        val kinds = skipKinds(id) ?: return emptyList()
        return attachments.filter { it.kind in kinds }.map { it.id }
    }

    fun clear(id: String, app: Application, gifs: GifRepository) {
        when (id) {
            "photos" -> { MediaFiles.clear(app, MediaCategory.PHOTOS); CoilSetup.clear(app) }
            "videos" -> { MediaFiles.clear(app, MediaCategory.VIDEOS); CoilSetup.clear(app) }
            "audio" -> MediaFiles.clear(app, MediaCategory.AUDIO)
            "temp" -> MediaFiles.clear(app, MediaCategory.TEMP)
            "other" -> MediaFiles.clear(app, MediaCategory.OTHER)
            "gifs" -> {
                gifs.clearMediaCache()
                Media3Cache.clear(app)
                CoilSetup.clear(app)
            }
            "links" -> {
                LinkPreviews.clear(app)
                CoilSetup.clear(app)
            }
            "all" -> {
                MediaFiles.clearAll(app)
                gifs.clearMediaCache()
                Media3Cache.clear(app)
                LinkPreviews.clear(app)
                CoilSetup.clear(app)
            }
        }
    }
}

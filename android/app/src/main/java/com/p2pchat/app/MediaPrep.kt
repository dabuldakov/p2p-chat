package com.p2pchat.app

import androidx.core.graphics.scale
import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.MediaMetadataRetriever
import android.net.Uri
import android.provider.OpenableColumns
import androidx.exifinterface.media.ExifInterface
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.effect.Presentation
import androidx.media3.transformer.Composition
import androidx.media3.transformer.EditedMediaItem
import androidx.media3.transformer.Effects
import androidx.media3.transformer.ExportException
import androidx.media3.transformer.ExportResult
import androidx.media3.transformer.Transformer
import androidx.media3.common.util.UnstableApi
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.File
import java.io.FileOutputStream
import kotlin.coroutines.resume

data class PreparedMedia(
    val id: String,
    val file: File,
    val mime: String,
    val size: Long,
    val width: Int,
    val height: Int,
    val durationMs: Long,
    val fileName: String,
    val thumbnail: ByteArray?,
)

@androidx.annotation.OptIn(UnstableApi::class)
object MediaPrep {
    private const val MAX_PHOTO = 20L * 1024 * 1024
    private const val MAX_VIDEO = 100L * 1024 * 1024
    private const val MAX_AUDIO = 30L * 1024 * 1024
    private const val MAX_VIDEO_MS = 10L * 60 * 1000
    private const val MAX_IMAGE_EDGE = 2048
    private const val THUMB_EDGE = 360

    suspend fun image(ctx: Context, uri: Uri): PreparedMedia? = withContext(Dispatchers.IO) {
        runCatching {
            val resolver = ctx.contentResolver
            val sizeHint = querySize(ctx, uri)
            if (sizeHint > 50L * 1024 * 1024) return@runCatching null

            val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
            resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, opts) }
            if (opts.outWidth <= 0 || opts.outHeight <= 0) return@runCatching null
            var sample = 1
            while (opts.outWidth / (sample * 2) >= MAX_IMAGE_EDGE &&
                opts.outHeight / (sample * 2) >= MAX_IMAGE_EDGE
            ) sample *= 2
            val decode = BitmapFactory.Options().apply { inSampleSize = sample }
            val src = resolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, decode) }
                ?: return@runCatching null

            val orientation = resolver.openInputStream(uri)?.use {
                runCatching { ExifInterface(it).rotationDegrees }.getOrDefault(0)
            } ?: 0
            val rotated = if (orientation != 0) {
                val m = Matrix().apply { postRotate(orientation.toFloat()) }
                Bitmap.createBitmap(src, 0, 0, src.width, src.height, m, true)
            } else src
            val scaled = scaleDown(rotated, MAX_IMAGE_EDGE)
            val outW = scaled.width
            val outH = scaled.height
            val out = MediaFiles.temp(ctx, "jpg")
            FileOutputStream(out).use { scaled.compress(Bitmap.CompressFormat.JPEG, 85, it) }
            if (scaled !== src) scaled.recycle()
            if (rotated !== src) rotated.recycle()
            src.recycle()
            if (out.length() > MAX_PHOTO) {
                out.delete()
                return@runCatching null
            }
            val id = MediaFiles.sha256(out)
            val file = MediaFiles.persist(out, id, "image/jpeg")
            val thumbBmp = BitmapFactory.decodeFile(file.absolutePath) ?: return@runCatching null
            val thumb = scaleDown(thumbBmp, THUMB_EDGE).let { bmp ->
                ByteArrayOutputStream().use { bos ->
                    bmp.compress(Bitmap.CompressFormat.JPEG, 80, bos)
                    if (bmp !== thumbBmp) bmp.recycle()
                    thumbBmp.recycle()
                    bos.toByteArray()
                }
            }
            PreparedMedia(
                id = id,
                file = file,
                mime = "image/jpeg",
                size = file.length(),
                width = outW,
                height = outH,
                durationMs = 0,
                fileName = queryName(ctx, uri) ?: "photo.jpg",
                thumbnail = thumb,
            )
        }.getOrNull()
    }

    suspend fun video(ctx: Context, uri: Uri): PreparedMedia? {
        val duration = metadataDuration(ctx, uri)
        if (duration > MAX_VIDEO_MS) return null
        return withContext(Dispatchers.IO) {
            runCatching {
                val out = MediaFiles.temp(ctx, "mp4")
                val ok = transcode(ctx, uri, out)
                if (!ok || !out.isFile || out.length() == 0L) {
                    out.delete()
                    copyToTemp(ctx, uri, "mp4") ?: return@runCatching null
                } else out
            }.getOrNull()?.let { file ->
                if (file.length() > MAX_VIDEO) {
                    file.delete()
                    null
                } else {
                    val id = MediaFiles.sha256(file)
                    val persisted = MediaFiles.persist(file, id, "video/mp4")
                    val (w, h, dur) = videoMeta(persisted)
                    PreparedMedia(
                        id = id,
                        file = persisted,
                        mime = "video/mp4",
                        size = persisted.length(),
                        width = w,
                        height = h,
                        durationMs = dur,
                        fileName = queryName(ctx, uri) ?: "video.mp4",
                        thumbnail = videoThumb(persisted),
                    )
                }
            }
        }
    }

    suspend fun audio(ctx: Context, uri: Uri): PreparedMedia? = withContext(Dispatchers.IO) {
        runCatching {
            val mime = ctx.contentResolver.getType(uri)?.takeIf { it.startsWith("audio/") } ?: "audio/mp4"
            val sizeHint = querySize(ctx, uri)
            if (sizeHint > MAX_AUDIO) return@runCatching null
            val temp = copyToTemp(ctx, uri, MediaFiles.extension(mime)) ?: return@runCatching null
            if (temp.length() > MAX_AUDIO) {
                temp.delete()
                return@runCatching null
            }
            val id = MediaFiles.sha256(temp)
            val file = MediaFiles.persist(temp, id, mime)
            val dur = metadataDuration(ctx, Uri.fromFile(file))
            PreparedMedia(
                id = id,
                file = file,
                mime = mime,
                size = file.length(),
                width = 0,
                height = 0,
                durationMs = dur,
                fileName = queryName(ctx, uri) ?: "audio.${MediaFiles.extension(mime)}",
                thumbnail = null,
            )
        }.getOrNull()
    }

    /** Записанный в приложении голос/видео: файл уже в нужном формате. */
    suspend fun recorded(ctx: Context, file: File, kind: String, mime: String): PreparedMedia? =
        withContext(Dispatchers.IO) {
            runCatching {
                if (!file.isFile || file.length() == 0L) return@runCatching null
                val id = MediaFiles.sha256(file)
                val persisted = MediaFiles.persist(file, id, mime)
                val (w, h, dur) = videoMeta(persisted)
                PreparedMedia(
                    id = id,
                    file = persisted,
                    mime = mime,
                    size = persisted.length(),
                    width = w,
                    height = h,
                    durationMs = dur,
                    fileName = if (kind == "voice") "voice.m4a" else "video.mp4",
                    thumbnail = if (kind == "video_note") videoThumb(persisted) else null,
                )
            }.getOrNull()
        }

    private suspend fun transcode(ctx: Context, uri: Uri, out: File): Boolean =
        withContext(Dispatchers.Main) {
            suspendCancellableCoroutine { cont ->
                val transformer = Transformer.Builder(ctx)
                    .setVideoMimeType(MimeTypes.VIDEO_H264)
                    .setAudioMimeType(MimeTypes.AUDIO_AAC)
                    .addListener(object : Transformer.Listener {
                        override fun onCompleted(composition: Composition, exportResult: ExportResult) {
                            if (cont.isActive) cont.resume(true)
                        }

                        override fun onError(
                            composition: Composition,
                            exportResult: ExportResult,
                            exception: ExportException,
                        ) {
                            if (cont.isActive) cont.resume(false)
                        }
                    })
                    .build()
                val item = EditedMediaItem.Builder(MediaItem.fromUri(uri))
                    .setEffects(Effects(emptyList(), listOf(Presentation.createForHeight(720))))
                    .build()
                runCatching { transformer.start(item, out.absolutePath) }
                    .onFailure { if (cont.isActive) cont.resume(false) }
                cont.invokeOnCancellation { runCatching { transformer.cancel() } }
            }
        }

    private fun copyToTemp(ctx: Context, uri: Uri, ext: String): File? = runCatching {
        val out = MediaFiles.temp(ctx, ext)
        ctx.contentResolver.openInputStream(uri)?.use { input ->
            FileOutputStream(out).use { output -> input.copyTo(output) }
        } ?: run {
            out.delete()
            return null
        }
        out
    }.getOrNull()

    private fun querySize(ctx: Context, uri: Uri): Long =
        ctx.contentResolver.query(uri, arrayOf(OpenableColumns.SIZE), null, null, null)?.use {
            if (it.moveToFirst()) it.getLong(0) else 0L
        } ?: 0L

    private fun queryName(ctx: Context, uri: Uri): String? =
        ctx.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0)?.takeIf(String::isNotBlank) else null
        }

    private fun metadataDuration(ctx: Context, uri: Uri): Long = runCatching {
        withRetriever { r ->
            r.setDataSource(ctx, uri)
            r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0L
        }
    }.getOrDefault(0L)

    private fun videoMeta(file: File): Triple<Int, Int, Long> = runCatching {
        withRetriever { r ->
            r.setDataSource(file.absolutePath)
            val w = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_WIDTH)?.toIntOrNull() ?: 0
            val h = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_VIDEO_HEIGHT)?.toIntOrNull() ?: 0
            val d = r.extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull() ?: 0L
            Triple(w, h, d)
        }
    }.getOrDefault(Triple(0, 0, 0))

    private fun videoThumb(file: File): ByteArray? = runCatching {
        withRetriever { r ->
            r.setDataSource(file.absolutePath)
            val frame = r.getFrameAtTime(1_000_000, MediaMetadataRetriever.OPTION_CLOSEST_SYNC)
                ?: return null
            val scaled = scaleDown(frame, THUMB_EDGE)
            ByteArrayOutputStream().use { bos ->
                scaled.compress(Bitmap.CompressFormat.JPEG, 80, bos)
                if (scaled !== frame) scaled.recycle()
                frame.recycle()
                bos.toByteArray()
            }
        }
    }.getOrNull()

    private inline fun <T> withRetriever(block: (MediaMetadataRetriever) -> T): T {
        val r = MediaMetadataRetriever()
        return try {
            block(r)
        } finally {
            runCatching { r.release() }
        }
    }

    private fun scaleDown(src: Bitmap, maxEdge: Int): Bitmap {
        val max = maxOf(src.width, src.height)
        if (max <= maxEdge) return src
        val scale = maxEdge.toFloat() / max
        return src.scale(
            (src.width * scale).toInt().coerceAtLeast(1),
            (src.height * scale).toInt().coerceAtLeast(1),
        )
    }

}

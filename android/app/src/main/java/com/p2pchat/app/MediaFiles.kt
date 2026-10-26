package com.p2pchat.app

import android.content.Context
import java.io.File
import java.io.FileInputStream
import java.security.MessageDigest

/** Файловый кэш медиа: имя файла = SHA-256 + расширение по MIME. */
object MediaFiles {
    fun dir(ctx: Context): File = File(ctx.filesDir, "media").apply { mkdirs() }

    fun temp(ctx: Context, suffix: String): File =
        File.createTempFile("media-", ".$suffix", dir(ctx))

    fun extension(mime: String): String = when (mime.lowercase()) {
        "image/jpeg" -> "jpg"
        "image/png" -> "png"
        "image/webp" -> "webp"
        "video/mp4" -> "mp4"
        "video/webm" -> "webm"
        "audio/mp4", "audio/aac", "audio/m4a" -> "m4a"
        "audio/mpeg" -> "mp3"
        "audio/ogg" -> "ogg"
        "audio/wav", "audio/x-wav" -> "wav"
        else -> "bin"
    }

    fun file(ctx: Context, id: String, mime: String): File =
        File(dir(ctx), "$id.${extension(mime)}")

    fun exists(ctx: Context, id: String, mime: String): Boolean =
        file(ctx, id, mime).let { it.isFile && it.length() > 0 }

    fun sha256(file: File): String {
        val md = MessageDigest.getInstance("SHA-256")
        FileInputStream(file).use { input ->
            val buf = ByteArray(64 * 1024)
            while (true) {
                val n = input.read(buf)
                if (n <= 0) break
                md.update(buf, 0, n)
            }
        }
        return md.digest().joinToString("") { "%02x".format(it) }
    }

    fun sha256(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    fun persist(file: File, id: String, mime: String): File {
        val dest = File(file.parentFile, "$id.${extension(mime)}")
        if (file.absolutePath != dest.absolutePath) {
            if (dest.exists()) dest.delete()
            check(file.renameTo(dest)) { "cannot persist media" }
        }
        return dest
    }
}

/** Категории файлового кэша медиа. */
enum class MediaCategory(val id: String) {
    PHOTOS("photos"),
    VIDEOS("videos"),
    AUDIO("audio"),
    TEMP("temp"),
    OTHER("other"),
}

fun MediaFiles.categoryOf(file: File): MediaCategory = when {
    file.name.endsWith(".part") || file.name.startsWith("media-") -> MediaCategory.TEMP
    file.extension.lowercase() in setOf("jpg", "jpeg", "png", "webp") -> MediaCategory.PHOTOS
    file.extension.lowercase() in setOf("mp4", "webm", "mov", "mkv") -> MediaCategory.VIDEOS
    file.extension.lowercase() in setOf("m4a", "mp3", "ogg", "wav", "aac") -> MediaCategory.AUDIO
    else -> MediaCategory.OTHER
}

fun MediaFiles.files(ctx: Context): List<File> =
    dir(ctx).listFiles()?.filter { it.isFile } ?: emptyList()

fun MediaFiles.stats(ctx: Context): Map<MediaCategory, Pair<Long, Int>> {
    val out = MediaCategory.entries.associateWith { 0L to 0 }.toMutableMap()
    for (f in files(ctx)) {
        val (size, count) = out[categoryOf(f)] ?: (0L to 0)
        out[categoryOf(f)] = (size + f.length()) to (count + 1)
    }
    return out
}

fun MediaFiles.size(ctx: Context, category: MediaCategory): Long =
    files(ctx).filter { categoryOf(it) == category }.sumOf { it.length() }

fun MediaFiles.count(ctx: Context, category: MediaCategory): Int =
    files(ctx).count { categoryOf(it) == category }

fun MediaFiles.clear(ctx: Context, category: MediaCategory) {
    files(ctx).filter { categoryOf(it) == category }.forEach { runCatching { it.delete() } }
}

fun MediaFiles.clearAll(ctx: Context) {
    files(ctx).forEach { runCatching { it.delete() } }
}

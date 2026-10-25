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

package com.p2pchat.app

import android.content.Context
import android.os.Handler
import android.os.Looper
import uniffi.p2pchat_ffi.ChatCore
import java.io.File
import java.io.RandomAccessFile
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

/**
 * Догрузка медиа по hash с любого участника группы.
 *
 * В сообщении едет только метадата; если файла нет, клиент рассылает REQUEST.
 * Любой участник, у которого файл есть, отдаёт его чанками. После сборки hash
 * проверяется, файл кладётся в MediaFiles.
 */
class MediaTransfer(
    private val ctx: Context,
    private val core: ChatCore,
    private val send: (List<ByteArray>, ByteArray) -> Unit,
    private val onReady: () -> Unit,
) {
    companion object {
        private const val TYPE_REQUEST = 1
        private const val TYPE_CHUNK = 2
        private const val HASH_LEN = 32
        private const val CHUNK = 32 * 1024
        private const val RETRY_MS = 30_000L
    }

    private class Pending(
        val group: ByteArray,
        val hash: String,
        val mime: String,
        val file: File,
    ) {
        var raf: RandomAccessFile? = null
        var total = -1L
    }

    private val pending = ConcurrentHashMap<String, Pending>()
    private val mimeByHash = ConcurrentHashMap<String, String>()
    private val requestedAt = HashMap<String, Long>()
    private val lock = Any()
    private val io = Executors.newSingleThreadExecutor()
    private val main = Handler(Looper.getMainLooper())

    fun ensure(group: ByteArray, hash: String, mime: String) {
        if (hash.isBlank() || MediaFiles.exists(ctx, hash, mime)) return
        mimeByHash[hash] = mime
        val key = group.toHex() + ":" + hash
        synchronized(lock) {
            val now = System.currentTimeMillis()
            if (now - (requestedAt[key] ?: 0L) < RETRY_MS) return
            requestedAt[key] = now
        }
        val frame = ByteArray(3 + group.size + HASH_LEN)
        val bb = ByteBuffer.wrap(frame).order(ByteOrder.BIG_ENDIAN)
        bb.put(TYPE_REQUEST.toByte())
        bb.putShort(group.size.toShort())
        bb.put(group)
        bb.put(hexBytes(hash))
        send(memberKeys(group), frame)
    }

    fun onFrame(from: ByteArray, frame: ByteArray) {
        if (frame.size < 3) return
        val bb = ByteBuffer.wrap(frame).order(ByteOrder.BIG_ENDIAN)
        val type = bb.get().toInt()
        val glen = bb.getShort().toInt() and 0xffff
        if (glen <= 0 || bb.remaining() < glen + HASH_LEN) return
        val group = ByteArray(glen).also { bb.get(it) }
        if (!core.groupsWith(from).contains(group)) return
        val hash = ByteArray(HASH_LEN).also { bb.get(it) }.toHex()
        when (type) {
            TYPE_REQUEST -> io.execute { serve(from, group, hash) }
            TYPE_CHUNK -> {
                if (bb.remaining() < 16) return
                val offset = bb.getLong()
                val total = bb.getLong()
                if (offset < 0 || total <= 0 || offset > total) return
                val data = ByteArray(bb.remaining()).also { bb.get(it) }
                io.execute { receive(group, hash, offset, total, data) }
            }
        }
    }

    private fun serve(to: ByteArray, group: ByteArray, hash: String) {
        val file = MediaFiles.dir(ctx).listFiles { f -> f.name.startsWith("$hash.") && !f.name.endsWith(".part") }
            ?.firstOrNull() ?: return
        if (!file.isFile || file.length() <= 0) return
        RandomAccessFile(file, "r").use { raf ->
            val total = raf.length()
            var offset = 0L
            while (offset < total) {
                val n = minOf(CHUNK.toLong(), total - offset).toInt()
                val data = ByteArray(n)
                raf.seek(offset)
                raf.readFully(data)
                send(listOf(to), chunkFrame(group, hash, offset, total, data))
                offset += n
            }
        }
    }

    private fun receive(group: ByteArray, hash: String, offset: Long, total: Long, data: ByteArray) {
        var p = pending[hash]
        if (p == null) {
            val mime = mimeByHash[hash] ?: findMime(hash) ?: "application/octet-stream"
            p = Pending(group, hash, mime, File(MediaFiles.dir(ctx), "$hash.part"))
                .also { pending[hash] = it }
        }
        synchronized(p) {
            if (p.total < 0) p.total = total
            if (p.total != total) return
            val raf = p.raf ?: RandomAccessFile(p.file, "rw").also { p.raf = it }
            raf.seek(offset)
            raf.write(data)
            if (offset + data.size >= total) {
                raf.close()
                p.raf = null
                if (MediaFiles.sha256(p.file) == hash) {
                    val dest = MediaFiles.file(ctx, hash, p.mime)
                    if (dest.exists()) dest.delete()
                    if (p.file.renameTo(dest)) main.post(onReady)
                } else {
                    p.file.delete()
                }
                pending.remove(hash)
            }
        }
    }

    private fun findMime(hash: String): String? {
        // MIME приходит в Attachment; здесь файл ищем по расширению.
        val f = MediaFiles.dir(ctx).listFiles { x -> x.name.startsWith("$hash.") }?.firstOrNull() ?: return null
        return when (f.extension) {
            "jpg", "jpeg" -> "image/jpeg"
            "png" -> "image/png"
            "webp" -> "image/webp"
            "mp4" -> "video/mp4"
            "m4a" -> "audio/mp4"
            "mp3" -> "audio/mpeg"
            "ogg" -> "audio/ogg"
            else -> null
        }
    }

    private fun memberKeys(group: ByteArray): List<ByteArray> {
        val me = core.publicKey()
        return core.members(group).map { it.publicKey }.filter { !it.contentEquals(me) }
    }

    private fun chunkFrame(
        group: ByteArray,
        hash: String,
        offset: Long,
        total: Long,
        data: ByteArray,
    ): ByteArray {
        val frame = ByteArray(3 + group.size + HASH_LEN + 16 + data.size)
        val bb = ByteBuffer.wrap(frame).order(ByteOrder.BIG_ENDIAN)
        bb.put(TYPE_CHUNK.toByte())
        bb.putShort(group.size.toShort())
        bb.put(group)
        bb.put(hexBytes(hash))
        bb.putLong(offset)
        bb.putLong(total)
        bb.put(data)
        return frame
    }

    private fun hexBytes(hex: String): ByteArray =
        ByteArray(hex.length / 2) { i -> hex.substring(i * 2, i * 2 + 2).toInt(16).toByte() }
}

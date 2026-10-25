package com.p2pchat.app

import android.content.Context
import android.media.MediaRecorder
import android.os.Build
import java.io.File

/** Запись голосового сообщения в M4A/AAC. */
class AudioRecorder(private val ctx: Context) {
    private var recorder: MediaRecorder? = null
    private var file: File? = null

    fun start(): Boolean {
        cancel()
        val out = MediaFiles.temp(ctx, "m4a")
        val r = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) MediaRecorder(ctx) else @Suppress("DEPRECATION") MediaRecorder()
        return runCatching {
            r.setAudioSource(MediaRecorder.AudioSource.MIC)
            r.setOutputFormat(MediaRecorder.OutputFormat.MPEG_4)
            r.setAudioEncoder(MediaRecorder.AudioEncoder.AAC)
            r.setAudioEncodingBitRate(96_000)
            r.setAudioSamplingRate(44_100)
            r.setOutputFile(out.absolutePath)
            r.prepare()
            r.start()
            recorder = r
            file = out
            true
        }.getOrElse {
            runCatching { r.release() }
            out.delete()
            false
        }
    }

    /** Остановить и вернуть файл. */
    fun stop(): File? {
        val r = recorder ?: return null
        recorder = null
        val out = file
        file = null
        runCatching { r.stop() }
        runCatching { r.release() }
        return out?.takeIf { it.isFile && it.length() > 0 }
    }

    fun cancel() {
        val r = recorder
        recorder = null
        if (r != null) {
            runCatching { r.stop() }
            runCatching { r.release() }
        }
        file?.delete()
        file = null
    }
}

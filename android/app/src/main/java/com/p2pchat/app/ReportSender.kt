package com.p2pchat.app

import java.util.Base64
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

internal object ReportSender {
    private val client = OkHttpClient()

    internal fun body(payload: ByteArray, signature: ByteArray): String {
        val report = Base64.getEncoder().encodeToString(payload)
        val sign = Base64.getEncoder().encodeToString(signature)
        return """{"report":"$report","signature":"$sign"}"""
    }

    fun send(url: String, payload: ByteArray, signature: ByteArray): Boolean = runCatching {
        val req = Request.Builder()
            .url(url)
            .post(body(payload, signature).toRequestBody("application/json; charset=utf-8".toMediaType()))
            .build()
        client.newCall(req).execute().use { it.isSuccessful }
    }.getOrDefault(false)
}

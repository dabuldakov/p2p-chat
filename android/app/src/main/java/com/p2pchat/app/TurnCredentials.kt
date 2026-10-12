package com.p2pchat.app

import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import org.webrtc.PeerConnection

/**
 * Получает временные TURN-креды с инфраструктурного эндпоинта (TURN REST API)
 * и собирает список ICE-серверов для WebRTC.
 *
 * Формат ответа:
 * { "username": "...", "credential": "...", "ttl": 3600,
 *   "uris": ["turn:host:3478?transport=udp", "..."] }
 */
object TurnCredentials {

    private val client = OkHttpClient()

    fun fetch(url: String): List<PeerConnection.IceServer>? {
        if (url.isBlank()) return null
        return try {
            val request = Request.Builder().url(url).build()
            client.newCall(request).execute().use { resp ->
                if (!resp.isSuccessful) return null
                val body = resp.body?.string() ?: return null
                val json = JSONObject(body)
                val uris = json.optJSONArray("uris")
                val username = json.optString("username").takeIf { it.isNotEmpty() }
                val credential = json.optString("credential").takeIf { it.isNotEmpty() }

                val servers = mutableListOf<PeerConnection.IceServer>()
                servers.add(
                    PeerConnection.IceServer.builder("stun:stun.l.google.com:19302").createIceServer()
                )
                if (uris != null) {
                    for (i in 0 until uris.length()) {
                        val url = uris.getString(i)
                        val b = PeerConnection.IceServer.builder(url)
                        if (username != null) b.setUsername(username)
                        if (credential != null) b.setPassword(credential)
                        servers.add(b.createIceServer())
                    }
                }
                servers
            }
        } catch (e: Exception) {
            null
        }
    }
}
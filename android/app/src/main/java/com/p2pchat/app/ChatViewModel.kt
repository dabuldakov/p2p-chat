package com.p2pchat.app

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.launch
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiMessage

/**
 * Состояние чата для Compose. Владеет Rust-ядром, signaling-клиентом и
 * WebRTC-транспортом.
 */
class ChatViewModel(app: Application) : AndroidViewModel(app) {

    companion object {
        /** Адрес инфраструктурного signaling-сервера (из BuildConfig). */
        val SIGNAL_URL = BuildConfig.SIGNAL_URL
    }

    private val core: ChatCore = ChatCore(":memory:")

    /** Наш публичный ключ — им обмениваемся с собеседником. */
    val publicKey: ByteArray = core.publicKey()

    /** Тот же ключ в hex — для показа, копирования и QR. */
    val publicKeyHex: String = publicKey.joinToString("") { "%02x".format(it) }

    private val signaling = SignalingClient(SIGNAL_URL, publicKey)
    private val transport = WebRtcTransport(app, core, signaling)

    private val group: ByteArray = "general".toByteArray()

    val peerId: String = core.peerIdHex()

    var messages by mutableStateOf<List<FfiMessage>>(emptyList())
        private set

    var peers by mutableStateOf(core.peers())
        private set

    var status by mutableStateOf("signaling…")
        private set

    init {
        signaling.connect()
        viewModelScope.launch {
            signaling.incoming.collect { env ->
                transport.onSignal(env)
                refresh()
            }
        }
    }

    /** Подключиться к собеседнику по hex его публичного ключа. */
    fun connectTo(peerPublicKeyHex: String) {
        val key = hexToBytes(peerPublicKeyHex)
        if (key == null) {
            status = "ключ должен быть hex"
            return
        }
        status = "connecting…"
        transport.connectTo(key)
    }

    fun createGroup(): ByteArray = core.createGroup(group)

    fun joinGroup(key: ByteArray) {
        core.joinGroup(group, key)
    }

    fun send(text: String) {
        if (text.isBlank()) return
        val envelope = core.post(group, text)
        transport.broadcast(envelope)
        refresh()
    }

    fun refresh() {
        messages = core.history(group)
        peers = core.peers()
    }

    override fun onCleared() {
        transport.close()
        signaling.close()
        super.onCleared()
    }
}

/** hex-строка → байты (null, если некорректна). */
internal fun hexToBytes(hex: String): ByteArray? {
    val clean = hex.trim().replace(" ", "")
    if (clean.isEmpty() || clean.length % 2 != 0) return null
    return runCatching {
        ByteArray(clean.length / 2) { i ->
            clean.substring(i * 2, i * 2 + 2).toInt(16).toByte()
        }
    }.getOrNull()
}
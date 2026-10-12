package com.p2pchat.app

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiMember
import uniffi.p2pchat_ffi.FfiMessage

/**
 * Состояние чата: ядро (Rust), signaling и WebRTC-транспорт.
 * Работает с несколькими группами; текущая группа выбирается в UI.
 */
class ChatViewModel(app: Application) : AndroidViewModel(app) {

    companion object {
        val SIGNAL_URL = BuildConfig.SIGNAL_URL
        val TURN_CREDS_URL = BuildConfig.TURN_CREDS_URL
    }

    private val core: ChatCore = ChatCore(app.filesDir.absolutePath + "/chat.db")

    val publicKey: ByteArray = core.publicKey()
    val publicKeyHex: String = publicKey.joinToString("") { "%02x".format(it) }
    val peerId: String = core.peerIdHex()

    private val signaling = SignalingClient(SIGNAL_URL, publicKey)
    private val transport = WebRtcTransport(app, signaling) { env -> handleIncoming(env) }

    var groups by mutableStateOf<List<String>>(emptyList())
        private set

    var currentGroup by mutableStateOf<ByteArray?>(null)
        private set

    var messages by mutableStateOf<List<FfiMessage>>(emptyList())
        private set

    var members by mutableStateOf<List<FfiMember>>(emptyList())
        private set

    var status by mutableStateOf("signaling…")
        private set

    var invite by mutableStateOf<String?>(null)
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

    // --- Группы ----------------------------------------------------------

    /** Создать группу с именем; возвращает инвайт (для показа/QR). */
    fun createGroup(name: String) {
        val g = name.ifBlank { "chat" }.toByteArray()
        invite = core.createGroupInvite(g)
        core.joinEnvelope(g)?.let { transport.broadcast(it) }
        refresh()
        selectGroup(g)
    }

    /** Вступить в группу по инвайт-строке. */
    fun joinGroup(inviteStr: String) {
        runCatching { core.joinFromInvite(inviteStr) }
            .onSuccess { g ->
                core.joinEnvelope(g)?.let { transport.broadcast(it) }
                refresh()
                selectGroup(g)
            }
            .onFailure { status = "плохой инвайт" }
    }

    fun selectGroup(g: ByteArray) {
        currentGroup = g
        invite = core.groupInvite(g)
        status = "connecting…"
        connectNeighbors(g)
        refresh()
    }

    fun backToList() {
        currentGroup = null
    }

    /** Подключиться к соседям текущей группы. */
    private fun connectNeighbors(g: ByteArray) {
        viewModelScope.launch {
            val ice = withContext(Dispatchers.IO) { TurnCredentials.fetch(TURN_CREDS_URL) }
            if (ice != null) transport.setIceServers(ice)
            val neighbors = core.neighbors(g)
            neighbors.forEach { pk -> runCatching { transport.connect(pk) } }
            status = "соседей: ${neighbors.size}"
        }
    }

    // --- Сообщения -------------------------------------------------------

    fun send(text: String) {
        val g = currentGroup ?: return
        if (text.isBlank()) return
        val envelope = core.post(g, text)
        transport.broadcast(envelope)
        refresh()
    }

    private fun handleIncoming(envelope: ByteArray) {
        val outgoing = core.receive(envelope)
        outgoing.forEach { transport.broadcast(it) }
        refresh()
    }

    fun refresh() {
        groups = core.listGroups().map { String(it) }.sorted()
        currentGroup?.let { g ->
            messages = core.history(g)
            members = core.members(g)
        }
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
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
 * Работает с несколькими группами; соединения строятся со соседями по всем группам.
 */
class ChatViewModel(app: Application) : AndroidViewModel(app) {

    companion object {
        val SIGNAL_URL = BuildConfig.SIGNAL_URL
        val TURN_CREDS_URL = BuildConfig.TURN_CREDS_URL
    }

    private val core: ChatCore = ChatCore(app.filesDir.absolutePath + "/chat.db")

    val publicKey: ByteArray = core.publicKey()
    val publicKeyHex: String = publicKey.toHex()
    val peerId: String = core.peerIdHex()

    var connected by mutableStateOf(0)
        private set

    private val signaling = SignalingClient(SIGNAL_URL, publicKey)
    private val transport = WebRtcTransport(app, publicKey, signaling, { env -> handleIncoming(env) }) { n ->
        connected = n
    }

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
            signaling.incoming.collect { env -> transport.onSignal(env) }
        }
        refresh()
        connectAll()
    }

    // --- Группы ----------------------------------------------------------

    fun createGroup(name: String) {
        val g = name.ifBlank { "chat" }.toByteArray()
        invite = core.createGroupInvite(g)
        core.joinEnvelope(g)?.let { transport.sendTargets(it.envelope, it.targets) }
        refresh()
        selectGroup(g)
        connectAll()
    }

    fun joinGroup(inviteStr: String) {
        runCatching { core.joinFromInvite(inviteStr) }
            .onSuccess { g ->
                core.joinEnvelope(g)?.let { transport.sendTargets(it.envelope, it.targets) }
                refresh()
                selectGroup(g)
                connectAll()
            }
            .onFailure { status = "плохой инвайт" }
    }

    fun selectGroup(g: ByteArray) {
        currentGroup = g
        invite = core.groupInvite(g)
        refresh()
    }

    fun backToList() {
        currentGroup = null
    }

    /** Соединиться с соседями по всем группам (объединение). */
    private fun connectAll() {
        viewModelScope.launch {
            val ice = withContext(Dispatchers.IO) { TurnCredentials.fetch(TURN_CREDS_URL) }
            if (ice != null) transport.setIceServers(ice)
            val keys = core.listGroups()
                .flatMap { core.neighborKeys(it) }
                .distinctBy { it.toHex() }
            keys.forEach { pk -> runCatching { transport.connect(pk) } }
            status = "соседей: ${keys.size}"
        }
    }

    // --- Сообщения -------------------------------------------------------

    fun send(text: String) {
        val g = currentGroup ?: return
        if (text.isBlank()) return
        val out = core.post(g, text)
        transport.sendTargets(out.envelope, out.targets)
        refresh()
    }

    private fun handleIncoming(envelope: ByteArray) {
        core.receive(envelope).forEach { out ->
            transport.sendTargets(out.envelope, out.targets)
        }
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

internal fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

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
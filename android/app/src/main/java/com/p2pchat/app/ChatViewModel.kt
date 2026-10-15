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
        private const val PAGE_SIZE = 50
    }

    private val core: ChatCore = ChatCore(app.filesDir.absolutePath + "/chat.db")

    val publicKey: ByteArray = core.publicKey()
    val publicKeyHex: String = publicKey.toHex()
    val peerId: String = core.peerIdHex()

    var connected by mutableStateOf(0)
        private set

    private val signaling = SignalingClient(SIGNAL_URL, publicKey)
    private val transport = WebRtcTransport(
        app,
        publicKey,
        signaling,
        { env -> handleIncoming(env) },
        onPeerChange = { n -> connected = n },
        onPeerConnected = { key -> onPeerConnected(key) },
    )

    var groups by mutableStateOf<List<String>>(emptyList())
        private set

    /** Непрочитанные по группам (имя группы → число). */
    var unread by mutableStateOf<Map<String, ULong>>(emptyMap())
        private set

    /** Число участников по группам (имя группы → число). */
    var memberCounts by mutableStateOf<Map<String, Int>>(emptyMap())
        private set

    var currentGroup by mutableStateOf<ByteArray?>(null)
        private set

    var messages by mutableStateOf<List<FfiMessage>>(emptyList())
        private set

    /** Есть ли ещё более старые сообщения для подгрузки. */
    var hasMore by mutableStateOf(false)
        private set

    var members by mutableStateOf<List<FfiMember>>(emptyList())
        private set

    var status by mutableStateOf("signaling…")
        private set

    var invite by mutableStateOf<String?>(null)
        private set

    /** Доступное обновление (версия новее установленной), если есть. */
    var update by mutableStateOf<AppUpdate?>(null)
        private set

    /** Черновик из системного «Поделиться»: подставляется в поле ввода группы. */
    var sharedDraft by mutableStateOf<String?>(null)
        private set

    /** Текущая версия приложения. */
    val version: String = BuildConfig.VERSION_NAME

    init {
        signaling.connect()
        viewModelScope.launch {
            signaling.incoming.collect { env -> transport.onSignal(env) }
        }
        refresh()
        connectAll()
        checkForUpdate()
        // периодический анти-энтропийный догон (офлайн-сообщения)
        viewModelScope.launch {
            while (true) {
                kotlinx.coroutines.delay(30_000)
                syncAll()
            }
        }
    }

    /** Проверить наличие новой релизной версии (один раз при открытии приложения). */
    private fun checkForUpdate() {
        viewModelScope.launch {
            update = withContext(Dispatchers.IO) { UpdateChecker.check(version) }
        }
    }

    /** Проверить обновления по требованию; сообщает, найдено ли обновление. */
    fun checkNow(onDone: (Boolean) -> Unit) {
        viewModelScope.launch {
            val info = withContext(Dispatchers.IO) { UpdateChecker.check(version) }
            update = info
            onDone(info != null)
        }
    }

    /** Догнать пропущенное с соседями по всем группам. */
    private fun syncAll() {
        for (g in core.listGroups()) {
            for (key in core.neighborKeys(g)) {
                runCatching { core.syncDigestTo(g, key) }.getOrNull()?.let {
                    transport.sendTargets(it.envelope, it.targets)
                }
            }
        }
    }

    /** Пир подключился — синхронизируем общие группы. */
    private fun onPeerConnected(peerKey: ByteArray) {
        for (g in core.groupsWith(peerKey)) {
            runCatching { core.syncDigestTo(g, peerKey) }.getOrNull()?.let {
                transport.sendTargets(it.envelope, it.targets)
            }
        }
    }

    // --- Группы ----------------------------------------------------------

    fun createGroup(name: String) {
        val n = name.trim()
        if (n.isEmpty()) return
        if (groups.contains(n)) {
            status = "группа «$n» уже существует"
            return
        }
        val g = n.toByteArray()
        invite = core.createGroupInvite(g)
        core.joinEnvelope(g)?.let { transport.sendTargets(it.envelope, it.targets) }
        refresh()
        selectGroup(g)
        connectAll()
    }

    fun joinGroup(inviteStr: String) {
        runCatching { core.joinFromInvite(inviteStr) }
            .onSuccess { join ->
                val g = join.groupId
                val host = join.hostKey
                if (host.isNotEmpty()) {
                    runCatching { transport.connectForce(host) }
                }
                // представимся в группе
                core.joinEnvelope(g)?.let { transport.sendTargets(it.envelope, it.targets) }
                // попросим историю у хозяина
                if (host.isNotEmpty()) {
                    runCatching { core.syncDigestTo(g, host) }.getOrNull()?.let {
                        transport.sendTargets(it.envelope, it.targets)
                    }
                }
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

    /** Пришёл текст из системного «Поделиться»: показать список групп и запомнить черновик. */
    fun onSharedText(text: String) {
        if (text.isBlank()) return
        sharedDraft = text
        currentGroup = null
    }

    /** Забрать черновик (одноразовый — после подстановки в поле ввода). */
    fun consumeSharedDraft(): String? {
        val draft = sharedDraft
        sharedDraft = null
        return draft
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

    /** Приложение вернулось на передний план — догоняем пропущенное. */
    fun onForeground() {
        refresh()
        connectAll()
        syncAll()
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
        val ids = core.listGroups()
        groups = ids.map { String(it) }.sorted()
        currentGroup?.let { g ->
            members = core.members(g)
            reloadLatest(g)
            // открытая группа считается прочитанной
            runCatching { core.markRead(g) }
        }
        unread = ids.associate { String(it) to core.unreadCount(it) }
        memberCounts = ids.associate { String(it) to core.members(it).size }
    }

    /** Загрузить последнюю страницу истории. */
    private fun reloadLatest(g: ByteArray) {
        val page = core.historyWindow(g, null, PAGE_SIZE.toUInt())
        messages = page.messages
        hasMore = page.hasMore
    }

    /** Подгрузить более старые сообщения. */
    fun loadOlder() {
        val g = currentGroup ?: return
        val oldest = messages.firstOrNull() ?: return
        val page = core.historyWindow(g, oldest.id, PAGE_SIZE.toUInt())
        if (page.messages.isNotEmpty()) {
            messages = page.messages + messages
        }
        hasMore = page.hasMore
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
package com.p2pchat.app

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiMember
import uniffi.p2pchat_ffi.FfiMessage

/**
 * Общепроцессный рантайм чата: ядро (Rust), signaling и WebRTC-транспорт.
 *
 * Живёт на уровне приложения (не ViewModel), чтобы соединения сохранялись,
 * когда экран закрыт, а foreground-служба держала процесс живым. UI (ViewModel)
 * только читает состояние и вызывает методы.
 */
object ChatRuntime {

    private const val PAGE_SIZE = 50
    private const val PREFS = "p2pchat"
    private const val KEY_BG = "background_enabled"
    private val SIGNAL_URL = BuildConfig.SIGNAL_URL
    private val TURN_CREDS_URL = BuildConfig.TURN_CREDS_URL

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)

    @Volatile
    private var initialized = false

    private lateinit var app: Application
    private lateinit var core: ChatCore
    private lateinit var signaling: SignalingClient
    private lateinit var transport: WebRtcTransport

    lateinit var publicKey: ByteArray
        private set
    lateinit var publicKeyHex: String
        private set
    lateinit var peerId: String
        private set

    val version: String get() = BuildConfig.VERSION_NAME

    var connected by mutableStateOf(0)
        private set
    var groups by mutableStateOf<List<String>>(emptyList())
        private set
    var unread by mutableStateOf<Map<String, ULong>>(emptyMap())
        private set
    var memberCounts by mutableStateOf<Map<String, Int>>(emptyMap())
        private set
    var currentGroup by mutableStateOf<ByteArray?>(null)
        private set
    var messages by mutableStateOf<List<FfiMessage>>(emptyList())
        private set
    var hasMore by mutableStateOf(false)
        private set
    var members by mutableStateOf<List<FfiMember>>(emptyList())
        private set
    var status by mutableStateOf("signaling…")
        private set
    var invite by mutableStateOf<String?>(null)
        private set
    var isOwner by mutableStateOf(false)
        private set
    var update by mutableStateOf<AppUpdate?>(null)
        private set
    var sharedDraft by mutableStateOf<String?>(null)
        private set

    /** Держать связь в фоне (foreground-служба). По умолчанию выключено. */
    var backgroundEnabled by mutableStateOf(false)
        private set

    /** Инициализация (идемпотентна). Вызывается из Application и ViewModel. */
    @Synchronized
    fun init(context: Application) {
        if (initialized) return
        initialized = true
        app = context

        core = ChatCore(context.filesDir.absolutePath + "/chat.db")
        publicKey = core.publicKey()
        publicKeyHex = publicKey.toHex()
        peerId = core.peerIdHex()

        signaling = SignalingClient(SIGNAL_URL, publicKey)
        transport = WebRtcTransport(
            context,
            publicKey,
            signaling,
            { env -> handleIncoming(env) },
            onPeerChange = { n -> connected = n },
            onPeerConnected = { key -> onPeerConnected(key) },
        )

        signaling.connect()
        scope.launch {
            signaling.incoming.collect { env -> transport.onSignal(env) }
        }
        refresh()
        connectAll()
        checkForUpdate()
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(30_000)
                syncAll()
            }
        }

        // восстановить настройку фона
        backgroundEnabled = app.getSharedPreferences(PREFS, 0).getBoolean(KEY_BG, false)
        if (backgroundEnabled) ChatService.start(app)
    }

    fun setBackground(enabled: Boolean) {
        backgroundEnabled = enabled
        app.getSharedPreferences(PREFS, 0).edit().putBoolean(KEY_BG, enabled).apply()
        if (enabled) ChatService.start(app) else ChatService.stop(app)
    }

    private fun checkForUpdate() {
        scope.launch {
            update = withContext(Dispatchers.IO) { UpdateChecker.check(version) }
        }
    }

    fun checkNow(onDone: (Boolean) -> Unit) {
        scope.launch {
            val info = withContext(Dispatchers.IO) { UpdateChecker.check(version) }
            update = info
            onDone(info != null)
        }
    }

    private fun syncAll() {
        for (g in core.listGroups()) {
            for (key in core.neighborKeys(g)) {
                runCatching { core.syncDigestTo(g, key) }.getOrNull()?.let {
                    transport.sendTargets(it.envelope, it.targets)
                }
            }
        }
    }

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
                if (host.isNotEmpty()) runCatching { transport.connectForce(host) }
                core.joinEnvelope(g)?.let { transport.sendTargets(it.envelope, it.targets) }
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
        isOwner = runCatching { core.isOwner(g) }.getOrDefault(false)
        refresh()
    }

    fun leaveGroup() {
        val g = currentGroup ?: return
        runCatching { core.leaveGroup(g) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
        currentGroup = null
        refresh()
    }

    fun removeMember(peerId: ByteArray) {
        val g = currentGroup ?: return
        runCatching { core.removeMember(g, peerId) }.getOrDefault(emptyList()).forEach {
            transport.sendTargets(it.envelope, it.targets)
        }
        refresh()
    }

    fun backToList() {
        currentGroup = null
    }

    fun onSharedText(text: String) {
        if (text.isBlank()) return
        sharedDraft = text
        currentGroup = null
    }

    fun consumeSharedDraft(): String? {
        val draft = sharedDraft
        sharedDraft = null
        return draft
    }

    private fun connectAll() {
        scope.launch {
            val ice = withContext(Dispatchers.IO) { TurnCredentials.fetch(TURN_CREDS_URL) }
            if (ice != null) transport.setIceServers(ice)
            val keys = core.listGroups()
                .flatMap { core.neighborKeys(it) }
                .distinctBy { it.toHex() }
            keys.forEach { pk -> runCatching { transport.connect(pk) } }
            status = "соседей: ${keys.size}"
        }
    }

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
            runCatching { core.markRead(g) }
        }
        unread = ids.associate { String(it) to core.unreadCount(it) }
        memberCounts = ids.associate { String(it) to core.members(it).size }
    }

    private fun reloadLatest(g: ByteArray) {
        val page = core.historyWindow(g, null, PAGE_SIZE.toUInt())
        messages = page.messages
        hasMore = page.hasMore
    }

    fun loadOlder() {
        val g = currentGroup ?: return
        val oldest = messages.firstOrNull() ?: return
        val page = core.historyWindow(g, oldest.id, PAGE_SIZE.toUInt())
        if (page.messages.isNotEmpty()) {
            messages = page.messages + messages
        }
        hasMore = page.hasMore
    }
}
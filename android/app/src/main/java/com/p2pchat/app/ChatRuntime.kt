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
import com.p2pchat.app.gif.Gif
import com.p2pchat.app.gif.GifCache
import com.p2pchat.app.gif.GifMediaCache
import com.p2pchat.app.gif.GifRef
import com.p2pchat.app.gif.GifRepository
import com.p2pchat.app.gif.KlipyApi
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiAttachment
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

    /** Виден ли UI сейчас (для бейджа: в фоне — со звуком, на экране — тихо). */
    @Volatile
    private var appVisible = false

    private lateinit var app: Application
    private lateinit var core: ChatCore
    private lateinit var signaling: SignalingClient
    private lateinit var transport: WebRtcTransport
    lateinit var gifs: GifRepository
        private set

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

    /** Никнеймы участников (hex peer_id → ник). */
    var nicknames by mutableStateOf<Map<String, String>>(emptyMap())
        private set

    var myNickname by mutableStateOf<String?>(null)
        private set

    /** Держать связь в фоне (foreground-служба). По умолчанию выключено. */
    var backgroundEnabled by mutableStateOf(false)
        private set

    /** Режим отладки (подробные логи). */
    var debugLogsEnabled by mutableStateOf(false)
        private set

    /** Тип соединения по каждому пиру (hex публичного ключа → "direct"/"turn"). */
    var peerKinds by mutableStateOf<Map<String, String>>(emptyMap())
        private set

    /** Сколько соединений «напрямую» и «через TURN» (для сводки). */
    val directCount: Int get() = peerKinds.values.count { it == "direct" }
    val turnCount: Int get() = peerKinds.values.count { it == "turn" }

    /** Время последней активности сигналинга (ms, unix). */
    var lastSignalingTs by mutableStateOf(0L)
        private set

    /** Инициализация (идемпотентна). Вызывается из Application и ViewModel. */
    @Synchronized
    fun init(context: Application) {
        if (initialized) return
        initialized = true
        app = context
        Logs.init(app)
        debugLogsEnabled = Logs.enabled

        core = ChatCore(context.filesDir.absolutePath + "/chat.db")
        publicKey = core.publicKey()
        publicKeyHex = publicKey.toHex()
        peerId = core.peerIdHex()
        Logs.i("mykey $publicKeyHex peer=$peerId")

        gifs = GifRepository(
            api = KlipyApi(BuildConfig.KLIPY_APP_KEY),
            cache = GifCache(java.io.File(app.filesDir, "gif_cache.json")),
            mediaCache = GifMediaCache(java.io.File(app.filesDir, "gif_media.json")),
        )

        signaling = SignalingClient(SIGNAL_URL, publicKey)
        transport = WebRtcTransport(
            context,
            publicKey,
            signaling,
            { env -> handleIncoming(env) },
            onPeerChange = { n -> connected = n },
            onPeerConnected = { key -> onPeerConnected(key) },
            onConnectionKind = { key, kind ->
                val h = key.toHex()
                peerKinds = if (kind == "offline") peerKinds - h else peerKinds + (h to kind)
            },
        )

        signaling.connect()
        scope.launch {
            signaling.incoming.collect { env -> runCatching { transport.onSignal(env) }.onFailure { Logs.w("signal error: $it") } }
        }
        refresh()
        connectAll()
        checkForUpdate()
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(15_000)
                connectAll()
                syncAll()
                lastSignalingTs = signaling.lastActivityMs
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

    /** Режим отладки: подробные логи в logcat (тег p2pchat). */
    fun setDebugLogs(enabled: Boolean) {
        Logs.setEnabled(app, enabled)
        debugLogsEnabled = enabled
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
        // заодно напоминаем свой ник
        runCatching { core.profileEnvelope() }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
        for (g in core.listGroups()) {
            for (key in core.neighborKeys(g)) {
                runCatching { core.syncDigestTo(g, key) }.getOrNull()?.let {
                    transport.sendTargets(it.envelope, it.targets)
                }
            }
        }
    }

    private fun onPeerConnected(peerKey: ByteArray) {
        runCatching { core.profileEnvelope() }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
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
            status = app.getString(R.string.group_exists, n)
            return
        }
        val g = n.toByteArray()
        invite = runCatching { core.createGroupInvite(g) }.getOrNull()
        refresh()
        selectGroup(g)
        connectAll()
    }

    fun joinGroup(inviteStr: String) {
        runCatching { core.joinFromInvite(inviteStr.trim()) }
            .onSuccess { join ->
                val host = join.hostKey
                if (host.isNotEmpty()) {
                    runCatching { transport.connectForce(host) }
                    // отправляем заявку (KeyPackage + билет) хозяину; ждём Welcome
                    transport.sendTargets(join.request.envelope, join.request.targets)
                    status = app.getString(R.string.joining)
                }
            }
            .onFailure { status = app.getString(R.string.bad_invite) }
    }

    fun selectGroup(g: ByteArray) {
        currentGroup = g
        invite = core.groupInvite(g)
        isOwner = runCatching { core.isOwner(g) }.getOrDefault(false)
        refresh()
        updateBadge(silent = true)
    }

    /** UI показан/скрыт (для бейджа). */
    fun setVisible(v: Boolean) {
        appVisible = v
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
            Logs.d("connectAll: groups=${core.listGroups().size} neighbors=${keys.size}")
            keys.forEach { pk ->
                Logs.d("neighbor ${pk.toHex()}")
                runCatching { transport.connect(pk) }
            }
            status = app.getString(R.string.neighbors, keys.size)
        }
    }

    fun onForeground() {
        appVisible = true
        lastSignalingTs = signaling.lastActivityMs
        refresh()
        connectAll()
        syncAll()
        updateBadge(silent = true)
    }

    /** Бейдж/уведомления: по одному на непрочитанную группу + сводное. */
    private fun updateBadge(silent: Boolean) {
        val items = groups.filter { (unread[it] ?: 0UL) > 0UL }.map { g ->
            val last = runCatching {
                core.historyWindow(g.toByteArray(), null, 1u).messages.lastOrNull()?.text
            }.getOrNull()
            g to (if (last.isNullOrBlank()) app.getString(R.string.new_messages) else last)
        }
        if (items.isEmpty()) Notifier.clearAll(app) else Notifier.showGroups(app, items, silent)
    }

    // --- Сообщения -------------------------------------------------------

    fun send(text: String) {
        val g = currentGroup ?: return
        if (text.isBlank()) return
        val out = core.post(g, text)
        Logs.d("send '${text.take(24)}' targets=${out.targets.size}")
        transport.sendTargets(out.envelope, out.targets)
        refresh()
    }

    /** Отправить гифку: между клиентами едет только ссылка (kind+id). */
    fun sendGif(gif: Gif) {
        val g = currentGroup ?: return
        val out = core.postGif(
            g,
            FfiAttachment(
                kind = "klipy",
                id = gif.providerId,
                width = (gif.width ?: 0).toUInt(),
                height = (gif.height ?: 0).toUInt(),
                url = "",
            ),
        )
        Logs.d("send gif ${gif.providerId} targets=${out.targets.size}")
        transport.sendTargets(out.envelope, out.targets)
        refresh()
    }

    // --- Гифки: пикер ----------------------------------------------------

    val gifEnabled: Boolean get() = gifs.enabled

    suspend fun searchGifs(query: String, page: Int): List<Gif> = gifs.search(query, page)

    suspend fun resolveGif(providerId: String): Gif? = gifs.resolve(providerId)

    fun cachedGif(providerId: String): Gif? = gifs.cached(providerId)

    /** Ссылки на отправленные мной гифки (вкладка «Мои»). */
    fun myGifRefs(limit: Int = 60): List<GifRef> =
        core.recentGifs(limit.toUInt()).map(::toGifRef)

    /** Ссылки на гифки текущей группы (вкладка «Эта группа»). */
    fun groupGifRefs(limit: Int = 60): List<GifRef> {
        val g = currentGroup ?: return emptyList()
        return core.groupGifs(g, limit.toUInt()).map(::toGifRef)
    }

    private fun toGifRef(a: FfiAttachment): GifRef =
        GifRef(
            providerId = a.id,
            width = a.width.toInt().takeIf { it > 0 },
            height = a.height.toInt().takeIf { it > 0 },
        )

    private fun handleIncoming(envelope: ByteArray) {
        val outs = runCatching { core.receive(envelope) }.getOrElse {
            Logs.w("receive error: $it")
            return
        }
        Logs.d("incoming env -> out=${outs.size}")
        outs.forEach { out ->
            runCatching { transport.sendTargets(out.envelope, out.targets) }
        }
        runCatching { refresh() }
        runCatching { updateBadge(silent = appVisible) }
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
        myNickname = core.myNickname()
        nicknames = core.allNicknames().associate { it.peerId.toHex() to it.nickname }
    }

    /** Задать свой никнейм и разослать его. */
    fun setNickname(name: String) {
        runCatching { core.setNickname(name.trim()) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
        refresh()
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
package com.p2pchat.app

import android.app.Application
import android.content.Intent
import android.net.Uri
import android.util.Base64
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import com.p2pchat.app.gif.Gif
import com.p2pchat.app.gif.GifCache
import com.p2pchat.app.gif.GifMediaCache
import com.p2pchat.app.gif.GifRef
import com.p2pchat.app.gif.GifRepository
import com.p2pchat.app.gif.KlipyApi
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiAttachment
import uniffi.p2pchat_ffi.FfiLink
import uniffi.p2pchat_ffi.FfiMember
import uniffi.p2pchat_ffi.FfiPeerLink
import uniffi.p2pchat_ffi.FfiMessage
import uniffi.p2pchat_ffi.FfiOwnerElection

/**
 * Общепроцессный рантайм чата: ядро (Rust), signaling и WebRTC-транспорт.
 *
 * Живёт на уровне приложения (не ViewModel), чтобы соединения сохранялись,
 * когда экран закрыт, а foreground-служба держала процесс живым. UI (ViewModel)
 * только читает состояние и вызывает методы.
 */
data class GraphEdge(val from: String, val to: String, val kind: String)

/** Группа: уникальный id + отдельное отображаемое имя. */
data class GroupInfo(val id: ByteArray, val name: String)

object ChatRuntime {

    private const val PAGE_SIZE = 50
    private const val PREFS = "p2pchat"
    private const val KEY_BG = "background_enabled"
    private val SIGNAL_URL = BuildConfig.SIGNAL_URL
    private val TURN_CREDS_URL = BuildConfig.TURN_CREDS_URL
    private val REPORT_URL = BuildConfig.REPORT_URL
    private val reportClient = OkHttpClient()

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
    var groupInfos by mutableStateOf<List<GroupInfo>>(emptyList())
        private set
    var unread by mutableStateOf<Map<String, ULong>>(emptyMap())
        private set
    var memberCounts by mutableStateOf<Map<String, Int>>(emptyMap())
        private set
    var currentGroup by mutableStateOf<ByteArray?>(null)
        private set
    var currentGroupName by mutableStateOf<String?>(null)
        private set
    var messages by mutableStateOf<List<FfiMessage>>(emptyList())
        private set
    var hasMore by mutableStateOf(false)
        private set
    var members by mutableStateOf<List<FfiMember>>(emptyList())
        private set
    var status by mutableStateOf("")
        private set
    var invite by mutableStateOf<String?>(null)
        private set

    /** Лимит входов текущего инвайта (1 — одноразовый, 5 — ссылка) и время генерации. */
    var inviteMaxUses by mutableStateOf<Int?>(null)
        private set
    var inviteAt by mutableStateOf<Long?>(null)
        private set
    var isOwner by mutableStateOf(false)
        private set

    /** peer_id владельца текущей группы (hex). */
    var ownerId by mutableStateOf<String?>(null)
        private set
    var update by mutableStateOf<AppUpdate?>(null)
        private set
    var sharedDraft by mutableStateOf<String?>(null)
        private set

    /** Никнеймы участников (hex peer_id → ник). */
    var nicknames by mutableStateOf<Map<String, String>>(emptyMap())
        private set

    /** Аватары участников (hex peer_id → изображение). */
    var avatars by mutableStateOf<Map<String, ByteArray>>(emptyMap())
        private set

    /** Аватар текущей группы. */
    var groupAvatarBytes by mutableStateOf<ByteArray?>(null)
        private set

    /** Аватары групп (hex group_id → изображение) для списка. */
    var groupAvatars by mutableStateOf<Map<String, ByteArray>>(emptyMap())
        private set

    var myNickname by mutableStateOf<String?>(null)
        private set

    /** Держать связь в фоне (foreground-служба). По умолчанию выключено. */
    var backgroundEnabled by mutableStateOf(false)
        private set

    /** Режим отладки (подробные логи). */
    var debugLogsEnabled by mutableStateOf(false)
        private set

    /** Последняя активность владельца текущей группы (ms, unix; 0 — нет данных). */
    var ownerLastSeen by mutableStateOf(0L)
        private set

    /** Когда мы последний раз отправляли heartbeat владельца (hex группы → ms). */
    private val ownerHeartbeatAt = HashMap<String, Long>()

    /** Активные выборы владельца текущей группы. */
    var ownerElection by mutableStateOf<FfiOwnerElection?>(null)
        private set

    /** Эпоха владельца текущей группы. */
    var ownerEpoch by mutableStateOf(0uL)
        private set

    /** Название группы после смены владельца (для системного баннера). */
    var ownerChangeNotice by mutableStateOf<String?>(null)
        private set

    private val ownerEpochSeen = HashMap<String, ULong>()

    /** Когда последний раз завершался вызов владельца (unix сек; 0 — не было). */
    var ownerChallengeAt by mutableStateOf(0uL)
        private set

    /** Не пытаться автоматически начать выборы чаще, чем раз в 10 минут. */
    private val ownerAutoAt = HashMap<String, Long>()

    /** Когда последний раз пересылали OwnerChange (hex группы → ms). */
    private val ownerReplayAt = HashMap<String, Long>()

    /** Подпись последнего показанного уведомления о выборах (hex группы → sig). */
    private val ownerNotified = HashMap<String, String>()

    /** Заблокированные (hex peer_id) — их сообщения скрыты. */
    var blocked by mutableStateOf<Set<String>>(emptySet())
        private set

    /** Тип соединения по каждому пиру (hex публичного ключа → "direct"/"turn"). */
    var peerKinds by mutableStateOf<Map<String, String>>(emptyMap())
        private set

    /** Сколько соединений «напрямую» и «через TURN» (для сводки). */
    val directCount: Int get() = peerKinds.values.count { it == "direct" }
    val turnCount: Int get() = peerKinds.values.count { it == "turn" }

    /** Соседи ТЕКУЩЕЙ группы (только её участники). */
    val groupNeighbors: Int
        get() {
            val g = currentGroup ?: return 0
            return runCatching { core.neighborKeys(g).distinctBy { it.toHex() }.size }.getOrDefault(0)
        }

    /** Сколько участников ТЕКУЩЕЙ группы реально подключено. */
    val groupConnected: Int
        get() {
            if (currentGroup == null) return connected
            return members.count { peerKinds[it.publicKey.toHex()] != null }
        }

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
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(15_000)
                connectAll()
                syncAll()
                maybeOwnerHeartbeat()
                maybeAdvanceOwnerElection()
                lastSignalingTs = signaling.lastActivityMs
            }
        }
        // публикация активных связей (эфемерно, раз в минуту)
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(60_000)
                runCatching { publishLinks() }
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

    private var lastUpdateCheck = 0L

    /** true — последняя попытка проверки обновлений не удалась (для UI/диагностики). */
    var updateCheckFailed by mutableStateOf(false)
        private set

    private fun checkForUpdate(force: Boolean = false) {
        if (BuildConfig.PLAY_BUILD) return // в Play обновления — через Play Store
        val now = System.currentTimeMillis()
        // Успешная проверка — не чаще раза в 30 мин, после сбоя — раз в 5 мин.
        val interval = if (updateCheckFailed) 5 * 60_000L else 30 * 60_000L
        if (!force && lastUpdateCheck != 0L && now - lastUpdateCheck < interval) return
        lastUpdateCheck = now
        Logs.i("update: checking installed=$version")
        scope.launch {
            val result = withContext(Dispatchers.IO) { UpdateChecker.check(version) }
            if (result.ok) {
                update = result.update
                updateCheckFailed = false
            } else {
                updateCheckFailed = true
            }
            Logs.i("update: installed=$version ok=${result.ok} latest=${result.update?.version}")
        }
    }

    fun checkNow(onDone: (Boolean) -> Unit) {
        if (BuildConfig.PLAY_BUILD) {
            val id = app.packageName
            val market = Intent(Intent.ACTION_VIEW, Uri.parse("market://details?id=$id"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            val web = Intent(Intent.ACTION_VIEW, Uri.parse("https://play.google.com/store/apps/details?id=$id"))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            runCatching { app.startActivity(market) }.onFailure { runCatching { app.startActivity(web) } }
            onDone(true) // диалог не откроется (update == null), просто уводим в Play
            return
        }
        lastUpdateCheck = System.currentTimeMillis()
        scope.launch {
            val result = withContext(Dispatchers.IO) { UpdateChecker.check(version) }
            if (result.ok) {
                update = result.update
                updateCheckFailed = false
            } else {
                updateCheckFailed = true
            }
            Logs.i("update(now): installed=$version ok=${result.ok} latest=${result.update?.version}")
            onDone(result.update != null)
        }
    }

    private fun syncAll() {
        // заодно напоминаем свой ник
        runCatching { core.profileEnvelope() }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
        val now = System.currentTimeMillis()
        for (g in core.listGroups()) {
            for (key in core.neighborKeys(g)) {
                runCatching { core.syncDigestTo(g, key) }.getOrNull()?.let {
                    transport.sendTargets(it.envelope, it.targets)
                }
            }
            val gkey = g.toHex()
            if (now - (ownerReplayAt[gkey] ?: 0L) > 60_000L) {
                runCatching { core.ownerChangeEnvelope(g) }.getOrNull()?.let {
                    transport.sendTargets(it.envelope, it.targets)
                }
                ownerReplayAt[gkey] = now
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
            runCatching { core.ownerChangeEnvelope(g) }.getOrNull()?.let {
                transport.sendTargets(it.envelope, it.targets)
            }
        }
    }

    /** Опубликовать свои активные связи по группам (эфемерно, не в лог). */
    fun publishLinks() {
        for (g in core.listGroups()) {
            val links = runCatching { core.members(g) }.getOrDefault(emptyList())
                .mapNotNull { m -> peerKinds[m.publicKey.toHex()]?.let { FfiPeerLink(m.peerId, it) } }
            if (links.isEmpty()) continue
            runCatching { core.postLinks(g, links) }.getOrNull()?.let {
                transport.sendTargets(it.envelope, it.targets)
            }
        }
    }

    /** Связи группы для графа (from, to, kind). */
    fun graphLinks(group: ByteArray): List<FfiLink> =
        runCatching { core.links(group) }.getOrDefault(emptyList())

    /**
     * Рёбра графа: свои связи — по «живому» типу (как в списке участников),
     * чужие — из опубликованных. Дедуп по паре; своё ребро приоритетно.
     */
    fun graphEdges(group: ByteArray): List<GraphEdge> {
        val me = peerId
        val out = LinkedHashMap<String, GraphEdge>()
        runCatching { core.members(group) }.getOrDefault(emptyList()).forEach { m ->
            val k = peerKinds[m.publicKey.toHex()] ?: return@forEach
            out[pairKey(me, m.peerId.toHex())] = GraphEdge(me, m.peerId.toHex(), k)
        }
        graphLinks(group).forEach { l ->
            val a = l.from.toHex(); val b = l.to.toHex()
            if (a == me || b == me) return@forEach
            out.putIfAbsent(pairKey(a, b), GraphEdge(a, b, l.kind))
        }
        return out.values.toList()
    }

    /** Все участники всех групп (уникальные по peer_id) — для общего графа. */
    fun allMembers(): List<FfiMember> {
        val out = LinkedHashMap<String, FfiMember>()
        for (g in core.listGroups()) {
            runCatching { core.members(g) }.getOrDefault(emptyList())
                .forEach { out[it.peerId.toHex()] = it }
        }
        return out.values.toList()
    }

    /** Рёбра графа по всем группам (дедуп по паре). */
    fun graphEdgesAll(): List<GraphEdge> {
        val out = LinkedHashMap<String, GraphEdge>()
        for (g in core.listGroups()) {
            graphEdges(g).forEach { out.putIfAbsent(pairKey(it.from, it.to), it) }
        }
        return out.values.toList()
    }

    private fun pairKey(a: String, b: String) = if (a < b) "$a|$b" else "$b|$a"

    // --- Группы ----------------------------------------------------------

    fun createGroup(name: String) {
        val n = name.trim()
        if (n.isEmpty()) return
        if (groupInfos.any { it.name.equals(n, ignoreCase = true) }) {
            val msg = app.getString(R.string.group_exists, n)
            status = msg
            Toast.makeText(app, msg, Toast.LENGTH_SHORT).show()
            return
        }
        val g = runCatching { core.createNamedGroup(n) }.getOrNull() ?: return
        invite = runCatching { core.groupInvite(g, 1u) }.getOrNull()
        inviteMaxUses = 1
        inviteAt = System.currentTimeMillis()
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
                    Toast.makeText(app, R.string.joining, Toast.LENGTH_SHORT).show()
                }
            }
            .onFailure {
                status = app.getString(R.string.bad_invite)
                Toast.makeText(app, R.string.bad_invite, Toast.LENGTH_SHORT).show()
            }
    }

    /** Пересоздать инвайт (maxUses=1 — одноразовый, 5 — «5 входов») и вернуть его. */
    fun regenInvite(maxUses: Int): String? {
        val g = currentGroup ?: return null
        val t = runCatching { core.groupInvite(g, maxUses.toUInt()) }.getOrNull()
        invite = t
        inviteMaxUses = maxUses
        inviteAt = System.currentTimeMillis()
        return t
    }

    fun selectGroup(g: ByteArray) {
        ownerChangeNotice = null
        currentGroup = g
        status = ""
        currentGroupName = runCatching { core.groupName(g) }.getOrDefault(String(g))
        invite = runCatching { core.groupInvite(g, 1u) }.getOrNull()
        inviteMaxUses = 1
        inviteAt = System.currentTimeMillis()
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
        currentGroupName = null
        refresh()
        updateBadge(silent = true)
    }

    fun removeMember(peerId: ByteArray) {
        val g = currentGroup ?: return
        runCatching { core.removeMember(g, peerId) }.getOrDefault(emptyList()).forEach {
            transport.sendTargets(it.envelope, it.targets)
        }
        refresh()
    }

    fun backToList() {
        ownerChangeNotice = null
        currentGroup = null
        currentGroupName = null
    }

    fun dismissOwnerChangeNotice() {
        ownerChangeNotice = null
    }

    fun onSharedText(text: String) {
        if (text.isBlank()) return
        sharedDraft = text
        currentGroup = null
        currentGroupName = null
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
            val turnN = ice?.count { s -> s.urls.any { it.startsWith("turn") } } ?: 0
            Logs.i("ice: creds=${ice != null} servers=${ice?.size ?: 0} turn=$turnN")
            val keys = core.listGroups()
                .flatMap { core.neighborKeys(it) }
                .distinctBy { it.toHex() }
            // Для статуса в экране группы показываем соседей ТОЛЬКО текущей группы,
            // а соединяемся со всеми (объединение по всем группам).
            val curCount = currentGroup?.let { g ->
                runCatching { core.neighborKeys(g).distinctBy { it.toHex() }.size }.getOrDefault(0)
            } ?: keys.size
            Logs.d("connectAll: groups=${core.listGroups().size} peers=${keys.size} groupNeighbors=$curCount")
            keys.forEach { pk ->
                Logs.d("neighbor ${pk.toHex()}")
                runCatching { transport.connect(pk) }
            }
            Logs.d("group neighbors: $curCount")
        }
    }

    fun onForeground() {
        appVisible = true
        lastSignalingTs = signaling.lastActivityMs
        refresh()
        connectAll()
        syncAll()
        updateBadge(silent = true)
        // Проверяем обновления при входе; при сбое — повтор (не чаще раза в 5 минут).
        checkForUpdate()
    }

    /** Бейдж/уведомления: по одному на непрочитанную группу + сводное. */
    private fun updateBadge(silent: Boolean) {
        val items = unreadGroups(groupInfos, unread).map { gi ->
            val last = runCatching {
                core.historyWindow(gi.id, null, 1u).messages.lastOrNull()?.text
            }.getOrNull()
            gi to notificationText(last, app.getString(R.string.new_messages))
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

    /** Поставить/снять реакцию на сообщение. */
    fun react(messageId: ByteArray, emoji: String, add: Boolean): Boolean {
        val g = currentGroup ?: return false
        val out = runCatching { core.react(g, messageId, emoji, add) }.getOrNull() ?: return false
        transport.sendTargets(out.envelope, out.targets)
        refresh()
        return true
    }

    /** Отредактировать своё сообщение (только автор). */
    fun editMessage(id: ByteArray, text: String) {
        val g = currentGroup ?: return
        if (text.isBlank()) return
        runCatching { core.editMessage(g, id, text) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
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

    private fun sameAvatars(a: Map<String, ByteArray>, b: Map<String, ByteArray>): Boolean {
        if (a.size != b.size) return false
        for ((k, v) in a) {
            val w = b[k] ?: return false
            if (!v.contentEquals(w)) return false
        }
        return true
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
        blocked = runCatching { core.blockedPeers() }.getOrDefault(emptyList())
            .map { it.toHex() }.toSet()
        groupInfos = ids
            .map { GroupInfo(it, runCatching { core.groupName(it) }.getOrDefault(String(it))) }
            .sortedBy { it.name.lowercase() }
        currentGroup?.let { g ->
            currentGroupName = runCatching { core.groupName(g) }.getOrDefault(String(g))
            // Владение могло перейти (выход владельца) — обновляем и в UI.
            isOwner = runCatching { core.isOwner(g) }.getOrDefault(false)
            ownerId = runCatching { core.ownerId(g)?.toHex() }.getOrNull()
            members = core.members(g)
            reloadLatest(g)
            // Помечаем прочитанным только если экран реально виден.
            if (shouldMarkRead(appVisible, true)) runCatching { core.markRead(g) }
        }
        unread = ids.associate { it.toHex() to core.unreadCount(it) }
        memberCounts = ids.associate { it.toHex() to core.members(it).size }
        myNickname = core.myNickname()
        val nicks = core.allNicknames().associate { it.peerId.toHex() to it.nickname }.toMutableMap()
        myNickname?.takeIf { it.isNotBlank() }?.let { nicks[peerId] = it }
        nicknames = nicks
        val freshAvatars = runCatching { core.avatars() }
            .getOrDefault(emptyList())
            .associate { it.peerId.toHex() to it.avatar }
        if (!sameAvatars(avatars, freshAvatars)) avatars = freshAvatars
        groupAvatars = ids.mapNotNull { g ->
            runCatching { core.groupAvatar(g) }.getOrNull()?.let { g.toHex() to it }
        }.toMap()
        ownerLastSeen = currentGroup?.let {
            runCatching { core.ownerLastSeen(it) }.getOrDefault(0uL).toLong() * 1000L
        } ?: 0L
        val elections = ids.associateBy(
            { it.toHex() },
            { runCatching { core.ownerElection(it) }.getOrNull() },
        )
        ownerElection = currentGroup?.let { elections[it.toHex()] }
        ownerEpoch = currentGroup?.let { runCatching { core.ownerEpoch(it) }.getOrDefault(0uL) } ?: 0uL
        currentGroup?.let { g ->
            val seen = ownerEpochSeen[g.toHex()]
            if (seen != null && ownerEpoch > seen) ownerChangeNotice = currentGroupName
            ownerEpochSeen[g.toHex()] = ownerEpoch
        }
        ownerChallengeAt = currentGroup?.let { runCatching { core.ownerChallengeAt(it) }.getOrDefault(0uL) } ?: 0uL
        groupInfos.forEach { gi -> maybeNotifyOwnerElection(gi, elections[gi.id.toHex()]) }
        val knownGroups = groupInfos.map { it.id.toHex() }.toSet()
        ownerNotified.keys.filter { it !in knownGroups }.forEach { clearOwnerNotice(it) }
        maybeOwnerHeartbeat()
        currentGroup?.let { g ->
            val fresh = runCatching { core.groupAvatar(g) }.getOrNull()
            val cur = groupAvatarBytes
            val same = (cur == null && fresh == null) ||
                (cur != null && fresh != null && cur.contentEquals(fresh))
            if (!same) groupAvatarBytes = fresh
        }
    }

    /** Задать свой аватар и разослать профиль. */
    fun setAvatar(bytes: ByteArray) {
        runCatching { core.setAvatar(bytes) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
        refresh()
    }

    private fun maybeOwnerHeartbeat() {
        val g = currentGroup ?: return
        if (!isOwner) return
        val key = g.toHex()
        val now = System.currentTimeMillis()
        if (now - (ownerHeartbeatAt[key] ?: 0L) < 12 * 60 * 60_000L) return
        ownerHeartbeatAt[key] = now
        runCatching { core.ownerAlive(g) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
    }

    private fun maybeNotifyOwnerElection(group: GroupInfo, election: FfiOwnerElection?) {
        val key = group.id.toHex()
        if (election == null) {
            clearOwnerNotice(key)
            return
        }
        if (appVisible && currentGroup?.contentEquals(group.id) == true) {
            clearOwnerNotice(key)
            return
        }
        val iAmClaimant = election.claims.any { it.claimant.toHex() == peerId }
        val iVoted = election.votes.any { it.voter.toHex() == peerId }
        val iAmOwner = runCatching { core.isOwner(group.id) }.getOrDefault(false)
        val ownerNeedsVote = iAmOwner && election.trigger.toInt() == 1
        if (iAmClaimant || iVoted || (iAmOwner && !ownerNeedsVote)) {
            clearOwnerNotice(key)
            return
        }
        val now = System.currentTimeMillis()
        val voteStarted = now >= election.voteStart.toLong() * 1000L
        val textRes = if (voteStarted) R.string.owner_notification_vote else R.string.owner_notification_start
        val sig = election.claims.joinToString(",") { it.claimId.toHex() } +
            ":" + election.trigger + ":" + voteStarted
        if (ownerNotified[key] == sig) return
        ownerNotified[key] = sig
        Notifier.showOwnerElection(app, key, group.name, app.getString(textRes))
    }

    private fun clearOwnerNotice(key: String) {
        ownerNotified.remove(key)
        Notifier.clearOwnerElection(app, key)
    }

    /** Подать заявку на владение: 0 — отсутствие, 1 — вызов, 2 — авто. */
    fun claimOwner(trigger: Int) {
        val g = currentGroup ?: return
        val out = runCatching { core.createOwnerClaim(g, trigger.toUInt()) }.getOrNull()
        if (out == null) {
            refresh()
            val msg = when {
                ownerElection != null -> R.string.owner_election_title
                members.size <= 2 -> R.string.owner_two_people_hint
                else -> R.string.owner_claim_unavailable
            }
            Toast.makeText(app, app.getString(msg), Toast.LENGTH_LONG).show()
            return
        }
        transport.sendTargets(out.envelope, out.targets)
        refresh()
    }

    /** Голос за заявку или за сохранение владельца. */
    fun voteOwner(claimId: ByteArray, approve: Boolean, keep: Boolean) {
        val g = currentGroup ?: return
        val out = runCatching { core.ownerVote(g, claimId, approve, keep) }.getOrNull()
        if (out == null) {
            Toast.makeText(app, app.getString(R.string.owner_vote_unavailable), Toast.LENGTH_SHORT).show()
            return
        }
        transport.sendTargets(out.envelope, out.targets)
        refresh()
    }

    /** Оставить текущего владельца (голос keep при вызове). */
    fun keepOwner() = voteOwner(ByteArray(0), true, true)

    private fun maybeAdvanceOwnerElection() {
        val g = currentGroup ?: return
        val now = System.currentTimeMillis()
        val election = runCatching { core.ownerElection(g) }.getOrNull()
        if (election != null) {
            if (now >= election.endsAt.toLong() * 1000L) {
                runCatching { core.finalizeOwnerElection(g) }.getOrNull()?.let {
                    transport.sendTargets(it.envelope, it.targets)
                }
                refresh()
            }
            return
        }
        if (isOwner || ownerLastSeen <= 0L) return
        if (now - ownerLastSeen < 8 * 86_400_000L) return
        val key = g.toHex()
        if (now - (ownerAutoAt[key] ?: 0L) < 10 * 60_000L) return
        ownerAutoAt[key] = now
        runCatching { core.createOwnerClaim(g, 2u) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
            refresh()
        }
    }

    fun blockPeer(peerId: ByteArray) {
        runCatching { core.blockPeer(peerId) }
        refresh()
    }

    fun unblockPeer(peerId: ByteArray) {
        runCatching { core.unblockPeer(peerId) }
        refresh()
    }

    /** Отправить подписанную жалобу на сервер (метаданные + скриншот). */
    fun sendReport(
        reportedPeer: ByteArray,
        groupId: ByteArray,
        messageId: ByteArray,
        reason: String,
        note: String,
        screenshot: ByteArray?,
        onResult: (Boolean) -> Unit,
    ) {
        val rep = runCatching {
            core.createReport(reportedPeer, groupId, messageId, reason, note, screenshot ?: ByteArray(0))
        }.getOrNull()
        if (rep == null) {
            onResult(false)
            return
        }
        scope.launch {
            val ok = withContext(Dispatchers.IO) {
                runCatching {
                    val body = JSONObject()
                        .put("report", Base64.encodeToString(rep.payload, Base64.NO_WRAP))
                        .put("signature", Base64.encodeToString(rep.signature, Base64.NO_WRAP))
                        .toString()
                    val req = Request.Builder()
                        .url(REPORT_URL)
                        .post(body.toRequestBody("application/json; charset=utf-8".toMediaType()))
                        .build()
                    reportClient.newCall(req).execute().use { it.isSuccessful }
                }.getOrDefault(false)
            }
            onResult(ok)
        }
    }

    /** Короткий отпечаток ключа для сверки. */
    fun fingerprint(): String = runCatching { core.fingerprint() }.getOrDefault("")

    /** Seed-фраза (24 слова) для бэкапа. */
    fun exportSeed(): String? = runCatching { core.exportSeed() }.getOrNull()

    /** Восстановить идентичность из фразы (только на пустом профиле). */
    fun importSeed(phrase: String): String? =
        runCatching { core.importSeed(phrase.trim()) }.getOrNull()

    /** Переименовать текущую группу (владелец). */
    fun renameGroup(name: String) {
        val g = currentGroup ?: return
        val n = name.trim()
        if (n.isBlank()) return
        runCatching { core.renameGroup(g, n) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
        refresh()
    }

    /** Удалить своё сообщение. */
    fun deleteMessage(id: ByteArray) {
        val g = currentGroup ?: return
        runCatching { core.deleteMessage(g, id) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
        refresh()
    }

    /** Сменить аватар группы (владелец). */
    fun setGroupAvatar(bytes: ByteArray) {
        val g = currentGroup ?: return
        runCatching { core.setGroupAvatar(g, bytes) }.getOrNull()?.let {
            transport.sendTargets(it.envelope, it.targets)
        }
        refresh()
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
package com.p2pchat.app

import androidx.core.content.edit
import android.annotation.SuppressLint
import android.app.Application
import android.widget.Toast
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import com.p2pchat.app.gif.Gif
import com.p2pchat.app.gif.GifCache
import com.p2pchat.app.gif.GifMediaCache
import com.p2pchat.app.gif.Media3Cache
import com.p2pchat.app.gif.CoilSetup
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
import uniffi.p2pchat_ffi.FfiOutgoing

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

data class StorageStat(val id: String, val size: Long, val count: Int = 0)

@SuppressLint("StaticFieldLeak")
object ChatRuntime {

    private const val PAGE_SIZE = 50
    private const val PREFS = "p2pchat"
    private const val KEY_BG = "background_enabled"
    private val SIGNAL_URL = BuildConfig.SIGNAL_URL
    private val REPORT_URL = BuildConfig.REPORT_URL

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
    private lateinit var media: MediaTransfer
    private lateinit var updates: UpdateController
    private lateinit var snapshots: SnapshotRepository
    private lateinit var gifRuntime: GifRuntime
    private lateinit var bridge: CoreBridge
    private lateinit var notifications: NotificationCoordinator
    private lateinit var owners: OwnerRuntime
    private lateinit var connections: ConnectionCoordinator
    lateinit var gifs: GifRepository
        private set

    lateinit var publicKey: ByteArray
        private set
    lateinit var publicKeyHex: String
        private set
    lateinit var peerId: String
        private set

    val version: String get() = BuildConfig.VERSION_NAME

    private val _uiState = MutableStateFlow(ChatUiState())
    val uiState: StateFlow<ChatUiState> = _uiState.asStateFlow()

    var connected by mutableIntStateOf(0)
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
    val update: AppUpdate? get() = updates.update
    val updateCheckFailed: Boolean get() = updates.checkFailed
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
    var ownerLastSeen by mutableLongStateOf(0L)
        private set

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

    /** Когда последний раз пересылали OwnerChange (hex группы → ms). */
    private val mediaSkip = mutableSetOf<String>()

    /** Заблокированные (hex peer_id) — их сообщения скрыты. */
    var blocked by mutableStateOf<Set<String>>(emptySet())
        private set

    /** Тип соединения по каждому пиру (hex публичного ключа → "direct"/"turn"). */
    var peerKinds by mutableStateOf<Map<String, String>>(emptyMap())
        private set

    /** Сколько соединений «напрямую» и «через TURN» (для сводки). */
    val directCount: Int get() = peerKinds.values.count { it == "direct" }
    val turnCount: Int get() = peerKinds.values.count { it == "turn" }

    /** Соседи ТЕКУЩЕЙ группы (только её участники), из последнего снимка. */
    private var currentGroupNeighbors by mutableIntStateOf(0)
    val groupNeighbors: Int get() = currentGroupNeighbors

    /** Сколько участников ТЕКУЩЕЙ группы реально подключено. */
    val groupConnected: Int
        get() {
            if (currentGroup == null) return connected
            return members.count { peerKinds[it.publicKey.toHex()] != null }
        }

    /** Время последней активности сигналинга (ms, unix). */
    var lastSignalingTs by mutableLongStateOf(0L)
        private set

    /** Инициализация (идемпотентна). Вызывается из Application и ViewModel. */
    @Synchronized
    fun init(context: Application) {
        if (initialized) return
        initialized = true
        app = context
        updates = UpdateController(context, scope) { version }
        Logs.init(app)
        debugLogsEnabled = Logs.enabled

        core = ChatCore(context.filesDir.absolutePath + "/chat.db")
        snapshots = SnapshotRepository(core, { peerId }, PAGE_SIZE)
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
            { env -> bridge.receive(env) },
            onPeerChange = { n -> connected = n },
            onPeerConnected = { key -> onPeerConnected(key) },
            onConnectionKind = { key, kind ->
                val h = key.toHex()
                peerKinds = if (kind == "offline") peerKinds - h else peerKinds + (h to kind)
            },
            onMediaFrame = { from, frame -> media.onFrame(from, frame) },
        )
        media = MediaTransfer(
            app,
            core,
            { targets, frame -> transport.sendMedia(targets, frame) },
            { key -> transport.mediaBufferedAmount(key) },
            { refresh() },
        )

        connections = ConnectionCoordinator(
            core = core,
            transport = transport,
            scope = scope,
            turnCredsUrl = BuildConfig.TURN_CREDS_URL,
            currentGroup = { currentGroup },
            peerKinds = { peerKinds },
        )
        bridge = CoreBridge(
            core = core,
            transport = transport,
            scope = scope,
            onDone = { refresh() },
            onReceived = { runCatching { notifications.updateBadge(silent = appVisible) } },
        )
        gifRuntime = GifRuntime(
            core = core,
            gifs = gifs,
            currentGroup = { currentGroup },
            post = { block -> bridge.out(block) },
        )
        notifications = NotificationCoordinator(
            app = app,
            core = core,
            scope = scope,
            groups = { groupInfos },
            unread = { unread },
            appVisible = { appVisible },
            currentGroup = { currentGroup },
            myPeerId = { peerId },
        )
        owners = OwnerRuntime(
            core = core,
            scope = scope,
            currentGroup = { currentGroup },
            isOwner = { isOwner },
            ownerLastSeen = { ownerLastSeen },
            ownerElection = { ownerElection },
            memberCount = { members.size },
            send = { out -> transport.sendTargets(out.envelope, out.targets) },
            refresh = { refresh() },
            toast = { res -> Toast.makeText(app, app.getString(res), Toast.LENGTH_SHORT).show() },
        )
        signaling.connect()
        scope.launch {
            signaling.incoming.collect { env -> runCatching { transport.onSignal(env) }.onFailure { Logs.w("signal error: $it") } }
        }
        refresh()
        connections.connectAll()
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(15_000)
                connections.connectAll()
                connections.syncAll()
                owners.heartbeat()
                owners.advance()
                currentGroup?.let { ensureMedia(it) }
                lastSignalingTs = signaling.lastActivityMs
            }
        }
        // публикация активных связей (эфемерно, раз в минуту)
        scope.launch {
            while (true) {
                kotlinx.coroutines.delay(60_000)
                runCatching { connections.publishLinks() }
            }
        }

        // восстановить настройку фона
        backgroundEnabled = app.getSharedPreferences(PREFS, 0).getBoolean(KEY_BG, false)
        if (backgroundEnabled) ChatService.start(app)

        _uiState.value = uiSnapshot()
        scope.launch {
            snapshotFlow { uiSnapshot() }.collect { _uiState.value = it }
        }
    }

    fun setBackground(enabled: Boolean) {
        backgroundEnabled = enabled
        app.getSharedPreferences(PREFS, 0).edit { putBoolean(KEY_BG, enabled) }
        if (enabled) ChatService.start(app) else ChatService.stop(app)
    }

    /** Режим отладки: подробные логи в logcat (тег p2pchat). */
    fun setDebugLogs(enabled: Boolean) {
        Logs.setEnabled(app, enabled)
        debugLogsEnabled = enabled
    }

    private fun checkForUpdate(force: Boolean = false) = updates.check(force)

    fun checkNow(onDone: (Boolean) -> Unit) = updates.checkNow(onDone)

    fun publishLinks() = connections.publishLinks()

    /** Связи группы для графа (from, to, kind). */
    fun graphLinks(group: ByteArray): List<FfiLink> =
        runCatching { core.links(group) }.getOrDefault(emptyList())

    /**
     * Рёбра графа: свои связи — по «живому» типу (как в списке участников),
     * чужие — из опубликованных. Дедуп по паре; своё ребро приоритетно.
     */
    fun graphEdges(group: ByteArray): List<GraphEdge> = buildGroupEdges(
        me = peerId,
        members = runCatching { core.members(group) }.getOrDefault(emptyList()),
        links = graphLinks(group),
        peerKinds = peerKinds,
    )

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
            graphEdges(g).forEach { out.putIfAbsent(graphPairKey(it.from, it.to), it) }
        }
        return out.values.toList()
    }

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
        scope.launch {
            val g = withContext(Dispatchers.IO) {
                runCatching { core.createNamedGroup(n) }.getOrNull()
            } ?: return@launch
            val inv = withContext(Dispatchers.IO) {
                runCatching { core.groupInvite(g, 1u) }.getOrNull()
            }
            invite = inv
            inviteMaxUses = 1
            inviteAt = System.currentTimeMillis()
            refresh()
            selectGroup(g)
            connections.connectAll()
        }
    }

    fun joinGroup(inviteStr: String) {
        scope.launch {
            val join = withContext(Dispatchers.IO) {
                runCatching { core.joinFromInvite(inviteStr.trim()) }.getOrNull()
            }
            if (join == null) {
                status = app.getString(R.string.bad_invite)
                Toast.makeText(app, R.string.bad_invite, Toast.LENGTH_SHORT).show()
                return@launch
            }
            val host = join.hostKey
            if (host.isNotEmpty()) {
                runCatching { transport.connectForce(host) }
                transport.sendTargets(join.request.envelope, join.request.targets)
                status = app.getString(R.string.joining)
                Toast.makeText(app, R.string.joining, Toast.LENGTH_SHORT).show()
            }
        }
    }

    /** Пересоздать инвайт (maxUses=1 — одноразовый, 5 — «5 входов») и вернуть его. */
    fun regenInvite(maxUses: Int, onResult: (String?) -> Unit) {
        val g = currentGroup ?: return onResult(null)
        scope.launch {
            val t = withContext(Dispatchers.IO) {
                runCatching { core.groupInvite(g, maxUses.toUInt()) }.getOrNull()
            }
            if (t != null) {
                invite = t
                inviteMaxUses = maxUses
                inviteAt = System.currentTimeMillis()
            }
            onResult(t)
        }
    }

    fun selectGroup(g: ByteArray) {
        ownerChangeNotice = null
        currentGroup = g
        status = ""
        scope.launch {
            val data = withContext(Dispatchers.IO) {
                Triple(
                    runCatching { core.groupName(g) }.getOrDefault(String(g)),
                    runCatching { core.groupInvite(g, 1u) }.getOrNull(),
                    runCatching { core.isOwner(g) }.getOrDefault(false),
                )
            }
            if (!sameGroup(g, currentGroup)) return@launch
            currentGroupName = data.first
            invite = data.second
            inviteMaxUses = 1
            inviteAt = System.currentTimeMillis()
            isOwner = data.third
            refresh()
            notifications.updateBadge(silent = true)
        }
    }

    /** UI показан/скрыт (для бейджа). */
    fun setVisible(v: Boolean) {
        appVisible = v
    }

    fun leaveGroup() {
        val g = currentGroup ?: return
        currentGroup = null
        currentGroupName = null
        scope.launch {
            val out = withContext(Dispatchers.IO) {
                runCatching { core.leaveGroup(g) }.getOrNull()
            }
            out?.let { transport.sendTargets(it.envelope, it.targets) }
            refresh()
            notifications.updateBadge(silent = true)
        }
    }

    fun removeMember(peerId: ByteArray) {
        val g = currentGroup ?: return
        bridge.outs { it.removeMember(g, peerId) }
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

    fun onForeground() {
        appVisible = true
        lastSignalingTs = signaling.lastActivityMs
        refresh()
        connections.connectAll()
        connections.syncAll()
        notifications.updateBadge(silent = true)
        // Проверяем обновления при входе; при сбое — повтор (не чаще раза в 5 минут).
        checkForUpdate()
    }

    /** Бейдж/уведомления: по одному на непрочитанную группу + сводное. */
    // --- Сообщения -------------------------------------------------------

    fun send(text: String) {
        val g = currentGroup ?: return
        if (text.isBlank()) return
        bridge.out { it.post(g, text) }
    }

    /** Поставить/снять реакцию на сообщение. */
    fun react(messageId: ByteArray, emoji: String, add: Boolean, onResult: (Boolean) -> Unit = {}) {
        val g = currentGroup ?: return onResult(false)
        scope.launch {
            val out = withContext(Dispatchers.IO) {
                runCatching { core.react(g, messageId, emoji, add) }.getOrNull()
            }
            if (out == null) {
                onResult(false)
                return@launch
            }
            transport.sendTargets(out.envelope, out.targets)
            refresh()
            onResult(true)
        }
    }

    /** Отредактировать своё сообщение (только автор). */
    fun editMessage(id: ByteArray, text: String) {
        val g = currentGroup ?: return
        if (text.isBlank()) return
        bridge.out { it.editMessage(g, id, text) }
    }

    private fun ensureMedia(group: ByteArray) {
        for (m in messages) {
            val a = m.attachment ?: continue
            if (a.kind in setOf("image", "video", "audio", "voice", "video_note") &&
                !MediaFiles.exists(app, a.id, a.mime) &&
                a.id !in mediaSkip
            ) {
                media.ensure(group, a.id, a.mime)
            }
        }
    }

    fun retryMedia(id: String, mime: String) {
        val g = currentGroup ?: return
        mediaSkip.remove(id)
        media.retry(g, id, mime)
    }

    /** Отправить подготовленное вложение. */
    fun sendPrepared(group: ByteArray, kind: String, p: PreparedMedia) {
        bridge.out {
            it.postAttachment(
                group,
                FfiAttachment(
                    kind = kind,
                    id = p.id,
                    width = p.width.toUInt(),
                    height = p.height.toUInt(),
                    url = "",
                    mime = p.mime,
                    size = p.size.toULong(),
                    durationMs = p.durationMs.toULong(),
                    fileName = p.fileName,
                    thumbnail = p.thumbnail ?: ByteArray(0),
                ),
            )
        }
    }

    fun storageStats(): List<StorageStat> = MediaStorage.stats(app, gifs)

    fun clearStorage(id: String) {
        MediaStorage.attachmentsToSkip(id, messages.mapNotNull { it.attachment })
            .forEach { mediaSkip.add(it) }
        MediaStorage.clear(id, app, gifs)
    }

    /** Отправить гифку: между клиентами едет только ссылка (kind+id). */
    fun sendGif(gif: Gif) = gifRuntime.send(gif)

    val gifEnabled: Boolean get() = gifRuntime.enabled

    suspend fun searchGifs(query: String, page: Int): List<Gif> = gifRuntime.search(query, page)
    suspend fun resolveGif(providerId: String): Gif? = gifRuntime.resolve(providerId)
    fun cachedGif(providerId: String): Gif? = gifRuntime.cached(providerId)
    fun myGifRefs(limit: Int = 60): List<GifRef> = gifRuntime.recentRefs(limit)
    fun groupGifRefs(limit: Int = 60): List<GifRef> = gifRuntime.groupRefs(limit)

    private fun onPeerConnected(peerKey: ByteArray) = connections.onPeerConnected(peerKey)

    private var refreshSeq = 0L

    private fun uiSnapshot() = ChatUiState(
        version = version,
        update = update,
        updateCheckFailed = updateCheckFailed,
        groupInfos = groupInfos,
        unread = unread,
        memberCounts = memberCounts,
        groupAvatars = groupAvatars,
        currentGroup = currentGroup,
        currentGroupName = currentGroupName,
        myPeerId = peerId,
        isOwner = isOwner,
        ownerId = ownerId,
        members = members,
        messages = messages,
        hasMore = hasMore,
        status = status,
        nicknames = nicknames,
        avatars = avatars,
        groupAvatar = groupAvatarBytes,
        peerKinds = peerKinds,
        connected = connected,
        groupConnected = groupConnected,
        groupNeighbors = currentGroupNeighbors,
        ownerLastSeen = ownerLastSeen,
        ownerElection = ownerElection,
        ownerChangeNotice = ownerChangeNotice,
        ownerChallengeAt = ownerChallengeAt,
        blocked = blocked,
        inviteMaxUses = inviteMaxUses,
        inviteAt = inviteAt,
        gifEnabled = gifEnabled,
        sharedDraft = sharedDraft,
        myNickname = myNickname,
        backgroundEnabled = backgroundEnabled,
        debugLogsEnabled = debugLogsEnabled,
        directCount = directCount,
        turnCount = turnCount,
        lastSignalingTs = lastSignalingTs,
    )

    fun refresh() {
        scope.launch {
            val seq = ++refreshSeq
            val group = currentGroup
            val snapshot = withContext(Dispatchers.IO) { snapshots.load(group, appVisible) }
            if (seq != refreshSeq) return@launch
            if (!sameGroup(group, currentGroup)) return@launch
            applySnapshot(snapshot)
        }
    }

    private fun applySnapshot(s: Snapshot) {
        blocked = s.blocked
        groupInfos = s.groupInfos
        currentGroupName = s.currentGroupName
        isOwner = s.isOwner
        ownerId = s.ownerId
        members = s.members
        messages = s.messages
        hasMore = s.hasMore
        if (!sameAvatars(avatars, s.avatars)) avatars = s.avatars
        unread = s.unread
        memberCounts = s.memberCounts
        myNickname = s.myNickname
        nicknames = s.nicknames
        groupAvatars = s.groupAvatars
        ownerLastSeen = s.ownerLastSeen
        ownerElection = s.group?.let { s.elections[it.toHex()] }
        ownerEpoch = s.ownerEpoch
        ownerChallengeAt = s.ownerChallengeAt
        currentGroupNeighbors = s.neighborCount
        s.group?.let { g ->
            val seen = ownerEpochSeen[g.toHex()]
            if (seen != null && s.ownerEpoch > seen) ownerChangeNotice = s.currentGroupName
            ownerEpochSeen[g.toHex()] = s.ownerEpoch
        }
        s.groupInfos.forEach { gi ->
            notifications.maybeNotifyOwnerElection(gi, s.elections[gi.id.toHex()], s.ownerFlags[gi.id.toHex()] == true)
        }
        val knownGroups = s.groupInfos.map { it.id.toHex() }.toSet()
        notifications.clearMissing(knownGroups)
        if (!sameBytes(groupAvatarBytes, s.groupAvatarBytes)) groupAvatarBytes = s.groupAvatarBytes
        s.group?.let { ensureMedia(it) }
        owners.heartbeat()
    }

    /** Задать свой аватар и разослать профиль. */
    fun setAvatar(bytes: ByteArray) {
        bridge.out { it.setAvatar(bytes) }
    }

    fun claimOwner(trigger: Int) = owners.claim(trigger)

    fun voteOwner(claimId: ByteArray, approve: Boolean, keep: Boolean) = owners.vote(claimId, approve, keep)

    fun keepOwner() = owners.keep()

    fun blockPeer(peerId: ByteArray) {
        scope.launch {
            withContext(Dispatchers.IO) { runCatching { core.blockPeer(peerId) } }
            refresh()
        }
    }

    fun unblockPeer(peerId: ByteArray) {
        scope.launch {
            withContext(Dispatchers.IO) { runCatching { core.unblockPeer(peerId) } }
            refresh()
        }
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
        scope.launch {
            val rep = withContext(Dispatchers.IO) {
                runCatching {
                    core.createReport(reportedPeer, groupId, messageId, reason, note, screenshot ?: ByteArray(0))
                }.getOrNull()
            }
            if (rep == null) {
                onResult(false)
                return@launch
            }
            val ok = withContext(Dispatchers.IO) {
                ReportSender.send(REPORT_URL, rep.payload, rep.signature)
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
        bridge.out { it.renameGroup(g, n) }
    }

    /** Удалить своё сообщение. */
    fun deleteMessage(id: ByteArray) {
        val g = currentGroup ?: return
        bridge.out { it.deleteMessage(g, id) }
    }

    /** Сменить аватар группы (владелец). */
    fun setGroupAvatar(bytes: ByteArray) {
        val g = currentGroup ?: return
        bridge.out { it.setGroupAvatar(g, bytes) }
    }

    /** Задать свой никнейм и разослать его. */
    fun setNickname(name: String) {
        bridge.out { it.setNickname(name.trim()) }
    }

    private fun reloadLatest(g: ByteArray) {
        val page = core.historyWindow(g, null, PAGE_SIZE.toUInt())
        messages = page.messages
        hasMore = page.hasMore
    }

    fun loadOlder() {
        val g = currentGroup ?: return
        val oldest = messages.firstOrNull() ?: return
        scope.launch {
            val page = withContext(Dispatchers.IO) {
                runCatching { core.historyWindow(g, oldest.id, PAGE_SIZE.toUInt()) }.getOrNull()
            } ?: return@launch
            if (!sameGroup(g, currentGroup)) return@launch
            if (page.messages.isNotEmpty()) messages = page.messages + messages
            ensureMedia(g)
            hasMore = page.hasMore
        }
    }
}

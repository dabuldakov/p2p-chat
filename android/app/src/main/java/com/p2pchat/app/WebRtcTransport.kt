package com.p2pchat.app

import android.content.Context
import android.os.Handler
import android.os.Looper
import com.p2pchat.proto.SignalEnvelope
import org.webrtc.CandidatePairChangeEvent
import org.webrtc.DataChannel
import org.webrtc.IceCandidate
import org.webrtc.MediaConstraints
import org.webrtc.MediaStream
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.RtpReceiver
import org.webrtc.RtpTransceiver
import org.webrtc.SdpObserver
import org.webrtc.SessionDescription
import java.nio.ByteBuffer
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Мультипировый WebRTC-транспорт с авто-реконнектом.
 *
 * По одному `RTCPeerConnection` на собеседника. Чтобы обе стороны не слали
 * offer одновременно (glare), инициатор — тот, чей публичный ключ лексикографически
 * меньше. Транспорт сам не знает про ядро: байты конвертов отдаёт через [onEnvelope],
 * а входящие рассылает вызовом [broadcast]/[sendTargets].
 */
class WebRtcTransport(
    context: Context,
    private val myKey: ByteArray,
    private val signaling: SignalingClient,
    private val onEnvelope: (ByteArray) -> Unit,
    private val onPeerChange: (Int) -> Unit = {},
    private val onPeerConnected: (ByteArray) -> Unit = {},
    private val onConnectionKind: (ByteArray, String) -> Unit = { _, _ -> },
    private val onMediaFrame: (ByteArray, ByteArray) -> Unit = { _, _ -> },
) : ChatTransport, EnvelopeSender {

    companion object {
        private fun defaultIceServers(): List<PeerConnection.IceServer> =
            listOf(PeerConnection.IceServer.builder("stun:stun.l.google.com:19302").createIceServer())
        private const val DATA_CHANNEL_LABEL = "p2pchat"
        private const val RECONNECT_DELAY_MS = 3000L
        private val MEDIA_MAGIC = byteArrayOf(0x50, 0x32, 0x50, 0x4D, 0x01)
        private const val RETRY_MS = 5000L
    }

    private class Conn(val remoteKey: ByteArray) {
        var pc: PeerConnection? = null
        var dc: DataChannel? = null
        var notified = false
        var dead = false
        val createdAt = System.currentTimeMillis()
        val pendingCandidates = mutableListOf<IceCandidate>()
        val pendingOutgoing = ArrayDeque<ByteArray>()
        val flushing = AtomicBoolean(false)
        val gathered = mutableMapOf<String, Int>()
    }

    private val factory: PeerConnectionFactory
    private val handler = Handler(Looper.getMainLooper())
    private val conns = ConcurrentHashMap<String, Conn>()
    private var closed = false

    @Volatile
    private var iceServers: List<PeerConnection.IceServer> = defaultIceServers()

    fun setIceServers(servers: List<PeerConnection.IceServer>) {
        if (servers.isNotEmpty()) iceServers = servers
    }

    init {
        PeerConnectionFactory.initialize(
            PeerConnectionFactory.InitializationOptions.builder(context)
                .setEnableInternalTracer(false)
                .createInitializationOptions()
        )
        factory = PeerConnectionFactory.builder().createPeerConnectionFactory()
    }

    /** Инициировать соединение (только если мы «инициатор» этой пары). */
    /** Установить/переустановить соединение с соседом (инициирует только меньший ключ). */
    fun connect(remoteKey: ByteArray) {
        val id = keyHex(remoteKey)
        val existing = conns[id]
        val isInitiator = PeerConnectionPolicy.weInitiate(myKey, remoteKey)
        val open = existing?.dc?.state() == DataChannel.State.OPEN
        val ageMs = existing?.let { System.currentTimeMillis() - it.createdAt } ?: 0L
        if (!PeerConnectionPolicy.shouldOffer(isInitiator, open, existing != null, ageMs, RETRY_MS)) {
            return
        }
        if (existing != null) {
            existing.dead = true
            conns.remove(id)
            handler.post { runCatching { existing.pc?.close() } }
        }
        Logs.d("connect -> offer ${id.take(8)}")
        startOffer(remoteKey)
    }

    /** Принудительное подключение (bootstrap-хост): игнорируем правило инициатора. */
    fun connectForce(remoteKey: ByteArray) {
        val id = keyHex(remoteKey)
        conns.remove(id)?.let {
            it.dead = true
            handler.post { runCatching { it.pc?.close() } }
        }
        Logs.d("connectForce -> offer ${id.take(8)}")
        startOffer(remoteKey)
    }

    /** Мы инициатор пары, если наш ключ лексикографически меньше. */
    private fun weInitiate(remoteKey: ByteArray): Boolean =
        PeerConnectionPolicy.weInitiate(myKey, remoteKey)

    fun onSignal(env: SignalEnvelope) {
        val from = env.from.toByteArray()
        val id = keyHex(from)
        Logs.d("signal ${env.bodyCase} from ${id.take(8)}")
        when (env.bodyCase) {
            SignalEnvelope.BodyCase.OFFER -> {
                var conn = conns[id]
                if (conn == null || conn.dead) {
                    conn = newConn(from)
                    conns[id] = conn
                }
                val desc = SessionDescription(SessionDescription.Type.OFFER, env.offer.sdp.toStringUtf8())
                conn.pc!!.setRemoteDescription(setRemoteObserver(conn), desc)
                conn.pc!!.createAnswer(sdpObserver(conn), MediaConstraints())
            }
            SignalEnvelope.BodyCase.ANSWER -> {
                val conn = conns[id] ?: return
                val desc = SessionDescription(SessionDescription.Type.ANSWER, env.answer.sdp.toStringUtf8())
                conn.pc!!.setRemoteDescription(setRemoteObserver(conn), desc)
            }
            SignalEnvelope.BodyCase.ICE -> {
                val conn = conns[id] ?: return
                val c = IceCandidate(env.ice.sdpMid, env.ice.sdpMlineIndex, env.ice.candidate)
                val pc = conn.pc
                if (pc != null && pc.remoteDescription != null) pc.addIceCandidate(c)
                else conn.pendingCandidates.add(c)
            }
            else -> Unit
        }
    }

    // --- ChatTransport ---------------------------------------------------

    override fun broadcast(envelope: ByteArray) {
        conns.values.forEach { sendTo(it, envelope) }
    }

    override fun sendTargets(envelope: ByteArray, targets: List<ByteArray>) {
        if (targets.isEmpty()) {
            broadcast(envelope)
            return
        }
        for (pk in targets) {
            val id = keyHex(pk)
            val conn = conns[id] ?: run { connect(pk); conns[id] }
            if (conn != null) sendTo(conn, envelope)
        }
    }

    /** Отправить бинарный media-фрейм (внутри того же DataChannel). */
    fun mediaBufferedAmount(target: ByteArray): Long {
        val conn = conns[keyHex(target)] ?: return 0L
        return runCatching { conn.dc?.bufferedAmount() ?: 0L }.getOrDefault(0L)
    }

    fun sendMedia(targets: List<ByteArray>, frame: ByteArray) {
        val bytes = ByteArray(MEDIA_MAGIC.size + frame.size)
        MEDIA_MAGIC.copyInto(bytes)
        frame.copyInto(bytes, MEDIA_MAGIC.size)
        handler.post { sendTargets(bytes, targets) }
    }

    override fun onIncoming(envelope: ByteArray) {
        onEnvelope(envelope)
    }

    fun connectedCount(): Int =
        conns.values.count { it.dc?.state() == DataChannel.State.OPEN }

    fun close() {
        closed = true
        conns.values.forEach { it.dc?.close(); it.pc?.close() }
        conns.clear()
        onPeerChange(0)
    }

    // --- Внутреннее ------------------------------------------------------

    private fun newConn(remoteKey: ByteArray): Conn {
        val conn = Conn(remoteKey)
        conn.pc = factory.createPeerConnection(iceServers, peerConnectionObserver(conn))
            ?: error("не удалось создать PeerConnection")
        return conn
    }

    private fun startOffer(remoteKey: ByteArray) {
        val conn = newConn(remoteKey)
        conns[keyHex(remoteKey)] = conn
        val dc = conn.pc!!.createDataChannel(DATA_CHANNEL_LABEL, DataChannel.Init())
        attachDataChannel(conn, dc)
        conn.pc!!.createOffer(sdpObserver(conn), MediaConstraints())
    }

    private fun handleDisconnect(conn: Conn) {
        if (conn.dead) return
        conn.dead = true
        val id = keyHex(conn.remoteKey)
        Logs.w("disconnect ${id.take(8)}")
        runCatching { onConnectionKind(conn.remoteKey, "offline") }
        // ВАЖНО: нельзя закрывать PeerConnection синхронно внутри native-callback
        // (реентрантность → SIGSEGV в libjingle). Откладываем на main-очередь.
        handler.post {
            if (conns[id] === conn) {
                conns.remove(id)
                runCatching { conn.pc?.close() }
                onPeerChange(connectedCount())
            }
        }
        if (!closed && weInitiate(conn.remoteKey)) {
            handler.postDelayed({
                if (!closed) startOffer(conn.remoteKey)
            }, RECONNECT_DELAY_MS)
        }
    }

    private fun sendTo(conn: Conn, envelope: ByteArray) {
        val dc = conn.dc
        Logs.d("send ${envelope.size}b dc=${dc?.state()} -> ${keyHex(conn.remoteKey).take(8)}")
        if (dc != null && dc.state() == DataChannel.State.OPEN) {
            dc.send(DataChannel.Buffer(ByteBuffer.wrap(envelope), true))
        } else {
            conn.pendingOutgoing.addLast(envelope)
        }
    }

    private fun attachDataChannel(conn: Conn, dc: DataChannel) {
        conn.dc = dc
        dc.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) = Unit
            override fun onStateChange() {
                // Никогда не даём исключению улететь в нативный колбэк (краш).
                runCatching {
                    if (dc.state() == DataChannel.State.OPEN) {
                        flushPending(conn)
                        if (!conn.notified) {
                            conn.notified = true
                            Logs.i("dc OPEN ${keyHex(conn.remoteKey).take(8)}")
                            onPeerConnected(conn.remoteKey)
                        }
                        probeConnectionKind(conn)
                    }
                    onPeerChange(connectedCount())
                }.onFailure { Logs.w("dc state error: $it") }
            }
            override fun onMessage(buffer: DataChannel.Buffer) {
                runCatching {
                    val bytes = ByteArray(buffer.data.remaining())
                    buffer.data.get(bytes)
                    Logs.d("dc recv ${bytes.size}b")
                    if (bytes.size >= MEDIA_MAGIC.size && MEDIA_MAGIC.indices.all { bytes[it] == MEDIA_MAGIC[it] }) {
                        onMediaFrame(conn.remoteKey, bytes.copyOfRange(MEDIA_MAGIC.size, bytes.size))
                    } else {
                        onIncoming(bytes)
                    }
                }.onFailure { Logs.w("dc recv error: $it") }
            }
        })
    }

    private fun flushPending(conn: Conn) {
        if (!conn.flushing.compareAndSet(false, true)) return
        try {
            val dc = conn.dc ?: return
            if (dc.state() != DataChannel.State.OPEN) return
            while (conn.pendingOutgoing.isNotEmpty()) {
                dc.send(DataChannel.Buffer(ByteBuffer.wrap(conn.pendingOutgoing.removeFirst()), true))
            }
        } finally {
            conn.flushing.set(false)
        }
    }

    private fun flushCandidates(conn: Conn) {
        val pc = conn.pc ?: return
        conn.pendingCandidates.forEach { pc.addIceCandidate(it) }
        conn.pendingCandidates.clear()
    }

    private fun sdpObserver(conn: Conn) = object : SdpObserver {
        override fun onCreateSuccess(sdp: SessionDescription) {
            val pc = conn.pc ?: return
            pc.setLocalDescription(noopObserver(), sdp)
            Logs.d("local ${sdp.type} created -> signal")
            when (sdp.type) {
                SessionDescription.Type.OFFER -> signaling.sendOffer(conn.remoteKey, sdp.description, sessionId())
                SessionDescription.Type.ANSWER -> signaling.sendAnswer(conn.remoteKey, sdp.description, sessionId())
                else -> Unit
            }
        }
        override fun onSetSuccess() = Unit
        override fun onCreateFailure(s: String?) = Unit
        override fun onSetFailure(s: String?) = Unit
    }

    private fun setRemoteObserver(conn: Conn) = object : SdpObserver {
        override fun onSetSuccess() = flushCandidates(conn)
        override fun onCreateSuccess(sdp: SessionDescription) = Unit
        override fun onCreateFailure(s: String?) = Unit
        override fun onSetFailure(s: String?) = Unit
    }

    private fun noopObserver() = object : SdpObserver {
        override fun onCreateSuccess(sdp: SessionDescription) = Unit
        override fun onSetSuccess() = Unit
        override fun onCreateFailure(s: String?) = Unit
        override fun onSetFailure(s: String?) = Unit
    }

    private fun peerConnectionObserver(conn: Conn) = object : PeerConnection.Observer {
        override fun onSignalingChange(state: PeerConnection.SignalingState) = Unit
        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState) {
            if (state == PeerConnection.IceConnectionState.FAILED ||
                state == PeerConnection.IceConnectionState.DISCONNECTED ||
                state == PeerConnection.IceConnectionState.CLOSED
            ) {
                runCatching { handleDisconnect(conn) }.onFailure { Logs.w("ice state error: $it") }
            }
        }
        override fun onStandardizedIceConnectionChange(state: PeerConnection.IceConnectionState) = Unit
        override fun onConnectionChange(state: PeerConnection.PeerConnectionState) {
            runCatching {
                if (state == PeerConnection.PeerConnectionState.FAILED ||
                    state == PeerConnection.PeerConnectionState.CLOSED
                ) {
                    handleDisconnect(conn)
                }
                onPeerChange(connectedCount())
            }.onFailure { Logs.w("conn change error: $it") }
        }
        override fun onIceConnectionReceivingChange(receiving: Boolean) = Unit
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState) {
            if (state == PeerConnection.IceGatheringState.COMPLETE) {
                val id = keyHex(conn.remoteKey).take(8)
                val g = conn.gathered
                Logs.i(
                    "ice gathered $id: host=${g["host"] ?: 0} srflx=${g["srflx"] ?: 0} " +
                        "prflx=${g["prflx"] ?: 0} relay=${g["relay"] ?: 0}",
                )
                if ((g["relay"] ?: 0) == 0 && !iceServers.any { s -> s.urls.any { it.startsWith("turn") } }) {
                    Logs.w("ice $id: нет relay-кандидатов (TURN недоступен/не настроен)")
                }
            }
        }
        override fun onIceCandidate(candidate: IceCandidate) {
            runCatching {
                val t = candidateType(candidate.sdp)
                if (t != null) conn.gathered[t] = (conn.gathered[t] ?: 0) + 1
                Logs.d("ice cand ${keyHex(conn.remoteKey).take(8)}: $t ${candidate.sdp.take(60)}")
                signaling.sendIce(conn.remoteKey, candidate.sdp, candidate.sdpMid, candidate.sdpMLineIndex)
            }.onFailure { Logs.w("send ice error: $it") }
        }
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>) = Unit
        override fun onAddStream(stream: MediaStream) = Unit
        override fun onRemoveStream(stream: MediaStream) = Unit
        override fun onDataChannel(dc: DataChannel) {
            runCatching { attachDataChannel(conn, dc) }.onFailure { Logs.w("onDataChannel error: $it") }
        }
        override fun onRenegotiationNeeded() = Unit
        override fun onAddTrack(receiver: RtpReceiver, mediaStreams: Array<out MediaStream>) = Unit
        override fun onRemoveTrack(receiver: RtpReceiver) = Unit
        override fun onTrack(transceiver: RtpTransceiver) = Unit
        override fun onSelectedCandidatePairChanged(event: CandidatePairChangeEvent) {
            probeConnectionKind(conn)
        }
    }

    private fun sessionId(): String = "s" + System.currentTimeMillis().toString(16)

    /** Определить тип соединения (напрямую / через TURN) по выбранной ICE-паре. */
    private fun probeConnectionKind(conn: Conn) {
        val pc = conn.pc ?: return
        runCatching {
            pc.getStats { report ->
                runCatching {
                    val stats = report.statsMap
                    var localId: String? = null
                    var remoteId: String? = null
                    for (s in stats.values) {
                        if (s.type == "candidate-pair") {
                            val m = s.members
                            val selected = (m["selected"] as? Boolean) ?: false
                            val succeeded = (m["state"] as? String) == "succeeded"
                            if (selected || succeeded) {
                                localId = m["localCandidateId"] as? String
                                remoteId = m["remoteCandidateId"] as? String
                            }
                        }
                    }
                    val type = localId?.let { stats[it]?.members?.get("candidateType") as? String }
                    val rtype = remoteId?.let { stats[it]?.members?.get("candidateType") as? String }
                    Logs.i(
                        "conn ${keyHex(conn.remoteKey).take(8)}: " +
                            "kind=${if (type == "relay") "turn" else type} local=$type remote=$rtype",
                    )
                    when (type) {
                        "relay" -> onConnectionKind(conn.remoteKey, "turn")
                        null -> {}
                        else -> onConnectionKind(conn.remoteKey, "direct")
                    }
                }
            }
        }.onFailure { Logs.w("stats error: $it") }
    }

    /** Тип ICE-кандидата из SDP-строки: host / srflx / prflx / relay. */
    private fun candidateType(sdp: String): String? {
        val i = sdp.indexOf(" typ ")
        if (i < 0) return null
        return sdp.substring(i + 5).trim().split(' ').firstOrNull()?.takeIf { it.isNotEmpty() }
    }

    private fun keyHex(k: ByteArray): String = PeerConnectionPolicy.keyHex(k)
}

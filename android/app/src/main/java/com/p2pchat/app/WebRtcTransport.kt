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
) : ChatTransport {

    companion object {
        private fun defaultIceServers(): List<PeerConnection.IceServer> =
            listOf(PeerConnection.IceServer.builder("stun:stun.l.google.com:19302").createIceServer())
        private const val DATA_CHANNEL_LABEL = "p2pchat"
        private const val RECONNECT_DELAY_MS = 3000L
        private const val RETRY_MS = 5000L
    }

    private class Conn(val remoteKey: ByteArray) {
        var pc: PeerConnection? = null
        var dc: DataChannel? = null
        var notified = false
        var makingOffer = false
        var dead = false
        val createdAt = System.currentTimeMillis()
        val pendingCandidates = mutableListOf<IceCandidate>()
        val pendingOutgoing = ArrayDeque<ByteArray>()
        val flushing = AtomicBoolean(false)
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
    /** Установить/переустановить соединение с соседом (обе стороны могут). */
    fun connect(remoteKey: ByteArray) {
        val id = keyHex(remoteKey)
        val existing = conns[id]
        if (existing != null) {
            if (existing.dc?.state() == DataChannel.State.OPEN) return
            if (System.currentTimeMillis() - existing.createdAt < RETRY_MS) return
            // соединение «зависло» — пересоздаём
            existing.dc?.close()
            existing.pc?.close()
            conns.remove(id)
        }
        android.util.Log.i("p2pchat", "connect -> offer ${id.take(8)}")
        startOffer(remoteKey)
    }

    /** «Вежливая» сторона при коллизии offer уступает (perfect negotiation). */
    private fun isPolite(remoteKey: ByteArray): Boolean = keyHex(myKey) > keyHex(remoteKey)

    fun onSignal(env: SignalEnvelope) {
        val from = env.from.toByteArray()
        val id = keyHex(from)
        android.util.Log.d("p2pchat", "signal ${env.bodyCase} from ${id.take(8)}")
        when (env.bodyCase) {
            SignalEnvelope.BodyCase.OFFER -> {
                val conn = conns.getOrPut(id) { newConn(from) }
                // perfect negotiation: при коллизии offer «невежливая» сторона игнорирует входящий
                if (conn.makingOffer && !isPolite(from)) {
                    android.util.Log.i("p2pchat", "glare: ignore offer ${id.take(8)}")
                    return
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

    fun sendTargets(envelope: ByteArray, targets: List<ByteArray>) {
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

    /** Принудительное подключение (bootstrap): то же, что connect. */
    fun connectForce(remoteKey: ByteArray) = connect(remoteKey)

    private fun newConn(remoteKey: ByteArray): Conn {
        val conn = Conn(remoteKey)
        conn.pc = factory.createPeerConnection(iceServers, peerConnectionObserver(conn))
            ?: error("не удалось создать PeerConnection")
        return conn
    }

    private fun startOffer(remoteKey: ByteArray) {
        val conn = newConn(remoteKey)
        conn.makingOffer = true
        conns[keyHex(remoteKey)] = conn
        val dc = conn.pc!!.createDataChannel(DATA_CHANNEL_LABEL, DataChannel.Init())
        attachDataChannel(conn, dc)
        conn.pc!!.createOffer(sdpObserver(conn), MediaConstraints())
    }

    private fun handleDisconnect(conn: Conn) {
        if (conn.dead) return
        conn.dead = true
        val id = keyHex(conn.remoteKey)
        android.util.Log.w("p2pchat", "disconnect ${id.take(8)}")
        if (conns[id] === conn) {
            conn.dc?.close()
            conn.pc?.close()
            conns.remove(id)
            onPeerChange(connectedCount())
        }
        if (!closed && !isPolite(conn.remoteKey)) {
            handler.postDelayed({
                if (!closed) startOffer(conn.remoteKey)
            }, RECONNECT_DELAY_MS)
        }
    }

    private fun sendTo(conn: Conn, envelope: ByteArray) {
        val dc = conn.dc
        android.util.Log.d("p2pchat", "send ${envelope.size}b dc=${dc?.state()} -> ${keyHex(conn.remoteKey).take(8)}")
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
                if (dc.state() == DataChannel.State.OPEN) {
                    flushPending(conn)
                    if (!conn.notified) {
                        conn.notified = true
                        android.util.Log.i("p2pchat", "dc OPEN ${keyHex(conn.remoteKey).take(8)}")
                        onPeerConnected(conn.remoteKey)
                    }
                }
                onPeerChange(connectedCount())
            }
            override fun onMessage(buffer: DataChannel.Buffer) {
                val bytes = ByteArray(buffer.data.remaining())
                buffer.data.get(bytes)
                android.util.Log.i("p2pchat", "dc recv ${bytes.size}b")
                onIncoming(bytes)
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
            conn.makingOffer = false
            val pc = conn.pc ?: return
            pc.setLocalDescription(noopObserver(), sdp)
            android.util.Log.i("p2pchat", "local ${sdp.type} created -> signal")
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
                handleDisconnect(conn)
            }
        }
        override fun onStandardizedIceConnectionChange(state: PeerConnection.IceConnectionState) = Unit
        override fun onConnectionChange(state: PeerConnection.PeerConnectionState) {
            if (state == PeerConnection.PeerConnectionState.FAILED ||
                state == PeerConnection.PeerConnectionState.CLOSED
            ) {
                handleDisconnect(conn)
            }
            onPeerChange(connectedCount())
        }
        override fun onIceConnectionReceivingChange(receiving: Boolean) = Unit
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState) = Unit
        override fun onIceCandidate(candidate: IceCandidate) {
            signaling.sendIce(conn.remoteKey, candidate.sdp, candidate.sdpMid, candidate.sdpMLineIndex)
        }
        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>) = Unit
        override fun onAddStream(stream: MediaStream) = Unit
        override fun onRemoveStream(stream: MediaStream) = Unit
        override fun onDataChannel(dc: DataChannel) = attachDataChannel(conn, dc)
        override fun onRenegotiationNeeded() = Unit
        override fun onAddTrack(receiver: RtpReceiver, mediaStreams: Array<out MediaStream>) = Unit
        override fun onRemoveTrack(receiver: RtpReceiver) = Unit
        override fun onTrack(transceiver: RtpTransceiver) = Unit
        override fun onSelectedCandidatePairChanged(event: CandidatePairChangeEvent) = Unit
    }

    private fun sessionId(): String = "s" + System.currentTimeMillis().toString(16)

    private fun keyHex(k: ByteArray): String = k.joinToString("") { "%02x".format(it) }
}
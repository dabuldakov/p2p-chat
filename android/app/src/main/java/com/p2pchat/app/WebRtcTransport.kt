package com.p2pchat.app

import android.content.Context
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
 * Мультипировый WebRTC-транспорт.
 *
 * Держит по одному `RTCPeerConnection` на каждого собеседника (мультиплекс по
 * каналу), поэтому работает и личка, и группы, где участник соединён с
 * несколькими соседями.
 *
 * Транспорт сам ничего не знает про ядро — он лишь передаёт байты конвертов
 * наружу через [onEnvelope] и рассылает входящие вызовом [broadcast].
 */
class WebRtcTransport(
    context: Context,
    private val signaling: SignalingClient,
    private val onEnvelope: (ByteArray) -> Unit,
) : ChatTransport {

    companion object {
        private fun defaultIceServers(): List<PeerConnection.IceServer> =
            listOf(PeerConnection.IceServer.builder("stun:stun.l.google.com:19302").createIceServer())
        private const val DATA_CHANNEL_LABEL = "p2pchat"
    }

    private class Conn(val remoteKey: ByteArray) {
        var pc: PeerConnection? = null
        var dc: DataChannel? = null
        val pendingCandidates = mutableListOf<IceCandidate>()
        val pendingOutgoing = ArrayDeque<ByteArray>()
        val flushing = AtomicBoolean(false)
    }

    private val factory: PeerConnectionFactory
    private val conns = ConcurrentHashMap<String, Conn>()

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

    /** Инициировать соединение с собеседником (по его публичному ключу). */
    fun connect(remoteKey: ByteArray) {
        val id = keyHex(remoteKey)
        if (conns.containsKey(id)) return
        val conn = Conn(remoteKey)
        conn.pc = factory.createPeerConnection(iceServers, peerConnectionObserver(conn))
            ?: error("не удалось создать PeerConnection")
        conns[id] = conn
        val dc = conn.pc!!.createDataChannel(DATA_CHANNEL_LABEL, DataChannel.Init())
        attachDataChannel(conn, dc)
        conn.pc!!.createOffer(sdpObserver(conn), MediaConstraints())
    }

    /** Обработать входящий сигнал (offer/answer/ice) от собеседника. */
    fun onSignal(env: SignalEnvelope) {
        val from = env.from.toByteArray()
        val id = keyHex(from)
        when (env.bodyCase) {
            SignalEnvelope.BodyCase.OFFER -> {
                val conn = conns.getOrPut(id) {
                    val c = Conn(from)
                    c.pc = factory.createPeerConnection(iceServers, peerConnectionObserver(c))
                    c
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

    /**
     * Отправить конверт адресатам (публичные ключи). Пустой список — всем.
     * С недостающими соседями соединение устанавливается на месте.
     */
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

    fun close() {
        conns.values.forEach { it.dc?.close(); it.pc?.close() }
        conns.clear()
    }

    // --- Внутреннее ------------------------------------------------------

    private fun sendTo(conn: Conn, envelope: ByteArray) {
        val dc = conn.dc
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
                if (dc.state() == DataChannel.State.OPEN) flushPending(conn)
            }
            override fun onMessage(buffer: DataChannel.Buffer) {
                val bytes = ByteArray(buffer.data.remaining())
                buffer.data.get(bytes)
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
            val pc = conn.pc ?: return
            pc.setLocalDescription(noopObserver(), sdp)
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
        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState) = Unit
        override fun onStandardizedIceConnectionChange(state: PeerConnection.IceConnectionState) = Unit
        override fun onConnectionChange(state: PeerConnection.PeerConnectionState) = Unit
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
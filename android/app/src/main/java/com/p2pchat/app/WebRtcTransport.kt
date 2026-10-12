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
import uniffi.p2pchat_ffi.ChatCore
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Реальный WebRTC-транспорт на libwebrtc.
 *
 * Управляет одним `RTCPeerConnection` с собеседником, открывает DataChannel
 * и гоняет через него конверты ядра. Signaling (`Offer`/`Answer`/`Ice`)
 * проходит через [SignalingClient].
 *
 * Чат-трафик после установления идёт напрямую (или через TURN при CGNAT).
 *
 * ВАЖНО: набор методов [PeerConnection.Observer] зависит от версии
 * webrtc-sdk. При смене зависимости сверьте сигнатуры.
 */
class WebRtcTransport(
    context: Context,
    private val core: ChatCore,
    private val signaling: SignalingClient,
) : ChatTransport {

    companion object {
        // Фолбэк, если не удалось получить временные TURN-креды (только STUN).
        private fun defaultIceServers(): List<PeerConnection.IceServer> =
            listOf(PeerConnection.IceServer.builder("stun:stun.l.google.com:19302").createIceServer())
        private const val DATA_CHANNEL_LABEL = "p2pchat"
    }

    @Volatile
    private var iceServers: List<PeerConnection.IceServer> = defaultIceServers()

    /** Задать ICE-серверы (например, с временными TURN-кредами). */
    fun setIceServers(servers: List<PeerConnection.IceServer>) {
        if (servers.isNotEmpty()) iceServers = servers
    }

    private val factory: PeerConnectionFactory

    @Volatile
    private var peerConnection: PeerConnection? = null

    @Volatile
    private var dataChannel: DataChannel? = null

    /** Публичный ключ собеседника — куда слать сигналы. */
    @Volatile
    private var remoteKey: ByteArray = ByteArray(0)

    private val pendingCandidates = mutableListOf<IceCandidate>()
    private val pendingOutgoing = ArrayDeque<ByteArray>()
    private val flushing = AtomicBoolean(false)

    init {
        PeerConnectionFactory.initialize(
            PeerConnectionFactory.InitializationOptions.builder(context)
                .setEnableInternalTracer(false)
                .createInitializationOptions()
        )
        factory = PeerConnectionFactory.builder().createPeerConnectionFactory()
    }

    /** Инициировать соединение: создаём канал и шлём offer. */
    fun connectTo(peerPublicKey: ByteArray) {
        remoteKey = peerPublicKey
        createPeerConnection()
        val dc = peerConnection!!.createDataChannel(DATA_CHANNEL_LABEL, DataChannel.Init())
        attachDataChannel(dc)
        peerConnection!!.createOffer(offerAnswerObserver(), MediaConstraints())
    }

    /** Обработать входящий сигнал (offer/answer/ice). */
    fun onSignal(env: SignalEnvelope) {
        when (env.bodyCase) {
            SignalEnvelope.BodyCase.OFFER -> {
                remoteKey = env.from.toByteArray()
                if (peerConnection == null) createPeerConnection()
                val desc = SessionDescription(
                    SessionDescription.Type.OFFER, env.offer.sdp.toStringUtf8()
                )
                peerConnection!!.setRemoteDescription(setRemoteObserver(), desc)
                peerConnection!!.createAnswer(offerAnswerObserver(), MediaConstraints())
            }
            SignalEnvelope.BodyCase.ANSWER -> {
                val desc = SessionDescription(
                    SessionDescription.Type.ANSWER, env.answer.sdp.toStringUtf8()
                )
                peerConnection!!.setRemoteDescription(setRemoteObserver(), desc)
            }
            SignalEnvelope.BodyCase.ICE -> {
                val c = IceCandidate(env.ice.sdpMid, env.ice.sdpMlineIndex, env.ice.candidate)
                val pc = peerConnection
                if (pc != null && pc.remoteDescription != null) pc.addIceCandidate(c)
                else pendingCandidates.add(c)
            }
            else -> Unit
        }
    }

    // --- ChatTransport ---------------------------------------------------

    override fun broadcast(envelope: ByteArray) {
        val dc = dataChannel
        if (dc != null && dc.state() == DataChannel.State.OPEN) {
            dc.send(DataChannel.Buffer(ByteBuffer.wrap(envelope), true))
        } else {
            pendingOutgoing.addLast(envelope)
        }
    }

    override fun onIncoming(envelope: ByteArray) {
        // Ядро применит входящий конверт и вернёт те, что надо переслать дальше.
        val outgoing = core.receive(envelope)
        outgoing.forEach { broadcast(it) }
    }

    fun close() {
        dataChannel?.close()
        peerConnection?.close()
        dataChannel = null
        peerConnection = null
    }

    // --- Внутреннее ------------------------------------------------------

    private fun createPeerConnection() {
        val pc = factory.createPeerConnection(iceServers, peerConnectionObserver())
            ?: error("не удалось создать PeerConnection")
        peerConnection = pc
    }

    private fun attachDataChannel(dc: DataChannel) {
        dataChannel = dc
        dc.registerObserver(object : DataChannel.Observer {
            override fun onBufferedAmountChange(previousAmount: Long) = Unit

            override fun onStateChange() {
                if (dc.state() == DataChannel.State.OPEN) flushPending()
            }

            override fun onMessage(buffer: DataChannel.Buffer) {
                val bytes = ByteArray(buffer.data.remaining())
                buffer.data.get(bytes)
                onIncoming(bytes)
            }
        })
    }

    private fun flushPending() {
        if (!flushing.compareAndSet(false, true)) return
        try {
            val dc = dataChannel ?: return
            if (dc.state() != DataChannel.State.OPEN) return
            while (pendingOutgoing.isNotEmpty()) {
                val bytes = pendingOutgoing.removeFirst()
                dc.send(DataChannel.Buffer(ByteBuffer.wrap(bytes), true))
            }
        } finally {
            flushing.set(false)
        }
    }

    private fun flushCandidates() {
        val pc = peerConnection ?: return
        pendingCandidates.forEach { pc.addIceCandidate(it) }
        pendingCandidates.clear()
    }

    private fun offerAnswerObserver() = object : SdpObserver {
        override fun onCreateSuccess(sdp: SessionDescription) {
            val pc = peerConnection ?: return
            pc.setLocalDescription(object : SdpObserver {
                override fun onCreateSuccess(sdp: SessionDescription) = Unit
                override fun onSetSuccess() = Unit
                override fun onCreateFailure(s: String?) = Unit
                override fun onSetFailure(s: String?) = Unit
            }, sdp)

            when (sdp.type) {
                SessionDescription.Type.OFFER ->
                    signaling.sendOffer(remoteKey, sdp.description, sessionId())
                SessionDescription.Type.ANSWER ->
                    signaling.sendAnswer(remoteKey, sdp.description, sessionId())
                else -> Unit
            }
        }

        override fun onSetSuccess() = Unit
        override fun onCreateFailure(s: String?) = Unit
        override fun onSetFailure(s: String?) = Unit
    }

    private fun setRemoteObserver() = object : SdpObserver {
        override fun onSetSuccess() = flushCandidates()
        override fun onCreateSuccess(sdp: SessionDescription) = Unit
        override fun onCreateFailure(s: String?) = Unit
        override fun onSetFailure(s: String?) = Unit
    }

    private fun peerConnectionObserver() = object : PeerConnection.Observer {
        override fun onSignalingChange(state: PeerConnection.SignalingState) = Unit
        override fun onIceConnectionChange(state: PeerConnection.IceConnectionState) = Unit
        override fun onStandardizedIceConnectionChange(state: PeerConnection.IceConnectionState) = Unit
        override fun onConnectionChange(state: PeerConnection.PeerConnectionState) = Unit
        override fun onIceConnectionReceivingChange(receiving: Boolean) = Unit
        override fun onIceGatheringChange(state: PeerConnection.IceGatheringState) = Unit

        override fun onIceCandidate(candidate: IceCandidate) {
            signaling.sendIce(remoteKey, candidate.sdp, candidate.sdpMid, candidate.sdpMLineIndex)
        }

        override fun onIceCandidatesRemoved(candidates: Array<out IceCandidate>) = Unit
        override fun onAddStream(stream: MediaStream) = Unit
        override fun onRemoveStream(stream: MediaStream) = Unit

        override fun onDataChannel(dc: DataChannel) {
            // Ответчик получает канал, созданный инициатором.
            attachDataChannel(dc)
        }

        override fun onRenegotiationNeeded() = Unit
        override fun onAddTrack(receiver: RtpReceiver, mediaStreams: Array<out MediaStream>) = Unit
        override fun onRemoveTrack(receiver: RtpReceiver) = Unit
        override fun onTrack(transceiver: RtpTransceiver) = Unit
        override fun onSelectedCandidatePairChanged(event: CandidatePairChangeEvent) = Unit
    }

    private fun sessionId(): String =
        "s" + System.currentTimeMillis().toString(16)
}
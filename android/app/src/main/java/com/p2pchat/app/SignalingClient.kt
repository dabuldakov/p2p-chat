package com.p2pchat.app

import android.os.Handler
import android.os.Looper
import com.google.protobuf.ByteString
import com.p2pchat.proto.Answer
import com.p2pchat.proto.IceCandidate
import com.p2pchat.proto.Offer
import com.p2pchat.proto.Register
import com.p2pchat.proto.SignalEnvelope
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString.Companion.toByteString
import java.security.MessageDigest
import java.util.concurrent.TimeUnit

/**
 * Клиент signaling поверх WebSocket. Общается с инфраструктурным сервером
 * (`p2pchat-signal-server`), который только маршрутизирует `Offer`/`Answer`/`Ice`.
 * Формат кадров — тот же `SignalEnvelope` (protobuf), что и в Rust.
 *
 * При обрыве соединения автоматически переподключается.
 */
class SignalingClient(
    private val url: String,
    private val publicKey: ByteArray,
) {
    private val client = OkHttpClient.Builder()
        .pingInterval(20, TimeUnit.SECONDS)
        .build()

    private val handler = Handler(Looper.getMainLooper())
    private var ws: WebSocket? = null
    private var closed = false

    private val _incoming = MutableSharedFlow<SignalEnvelope>(extraBufferCapacity = 128)
    val incoming = _incoming.asSharedFlow()

    fun connect() {
        closed = false
        open()
    }

    private fun open() {
        val request = Request.Builder().url(url).build()
        ws = client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                Logs.i("ws open: $url")
                webSocket.send(registerEnvelope().toByteArray().toByteString())
            }

            override fun onMessage(webSocket: WebSocket, bytes: okio.ByteString) {
                Logs.d("ws recv ${bytes.size}b")
                runCatching { SignalEnvelope.parseFrom(bytes.toByteArray()) }
                    .onSuccess { env ->
                        Logs.d("ws msg ${env.bodyCase} from ${env.from.toByteArray().joinToString(""){"%02x".format(it)}.take(8)}")
                        _incoming.tryEmit(env)
                    }
                    .onFailure { Logs.w("ws parse fail: $it") }
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                Logs.w("ws failure: $t")
                scheduleReconnect()
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                Logs.w("ws closed $code $reason")
                scheduleReconnect()
            }
        })
    }

    private fun scheduleReconnect() {
        if (closed) return
        handler.postDelayed({ if (!closed) open() }, 3000)
    }

    fun sendOffer(to: ByteArray, sdp: String, sessionId: String) {
        send(build(to) {
            it.offer = Offer.newBuilder()
                .setSdp(ByteString.copyFromUtf8(sdp))
                .setSessionId(sessionId)
                .build()
        })
    }

    fun sendAnswer(to: ByteArray, sdp: String, sessionId: String) {
        send(build(to) {
            it.answer = Answer.newBuilder()
                .setSdp(ByteString.copyFromUtf8(sdp))
                .setSessionId(sessionId)
                .build()
        })
    }

    fun sendIce(to: ByteArray, candidate: String, sdpMid: String, mline: Int) {
        send(build(to) {
            it.ice = IceCandidate.newBuilder()
                .setCandidate(candidate)
                .setSdpMid(sdpMid)
                .setSdpMlineIndex(mline)
                .build()
        })
    }

    fun send(env: SignalEnvelope) {
        ws?.send(env.toByteArray().toByteString())
    }

    fun close() {
        closed = true
        ws?.close(1000, "bye")
        ws = null
    }

    private fun build(to: ByteArray, setBody: (SignalEnvelope.Builder) -> Unit): SignalEnvelope {
        val b = SignalEnvelope.newBuilder().setFrom(ByteString.copyFrom(publicKey))
        if (to.isNotEmpty()) b.to = ByteString.copyFrom(to)
        setBody(b)
        return b.build()
    }

    private fun registerEnvelope(): SignalEnvelope =
        SignalEnvelope.newBuilder()
            .setFrom(ByteString.copyFrom(publicKey))
            .setRegister(
                Register.newBuilder()
                    .setPeerId(ByteString.copyFrom(sha256(publicKey)))
                    .build()
            )
            .build()
}

internal fun sha256(data: ByteArray): ByteArray =
    MessageDigest.getInstance("SHA-256").digest(data)
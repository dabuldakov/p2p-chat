package com.p2pchat.app

import uniffi.p2pchat_ffi.ChatCore

/**
 * Абстракция транспорта на стороне Android.
 *
 * Ядро (`ChatCore`) отдаёт и принимает «конверты» — непрозрачные байты.
 * Реальная доставка — здесь. На этот интерфейс ложится WebRTC-транспорт
 * (Google WebRTC SDK: RTCPeerConnection + DataChannel) и signaling
 * (WebSocket к инфраструктурному серверу).
 */
interface ChatTransport {
    /** Отправить конверт всем известным пирам (gossip-пересылка). */
    fun broadcast(envelope: ByteArray)

    /** Отдать входящий конверт в ядро и разослать порождённые ответы. */
    fun onIncoming(envelope: ByteArray)
}

/**
 * Заглушка-«лупбэк» для демонстрации UI без сети: ничего не отправляет,
 * но умеет принимать (например, из теста или от другого модуля).
 */
class LoopbackTransport(private val core: ChatCore) : ChatTransport {
    override fun broadcast(envelope: ByteArray) {
        // В реальном приложении: WebRtcTransport.broadcast(envelope)
    }

    override fun onIncoming(envelope: ByteArray) {
        // Ядро вернёт конверты, которые нужно переслать соседям.
        core.receive(envelope)
    }
}
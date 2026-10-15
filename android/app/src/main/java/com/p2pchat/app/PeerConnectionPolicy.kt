package com.p2pchat.app

/**
 * Чистая (не зависящая от WebRTC) политика установления соединений.
 *
 * Чтобы две стороны не слали `offer` одновременно (glare) — а Android-WebRTC
 * не поддерживает `rollback`, из-за чего ответ не создаётся и DataChannel
 * навсегда остаётся в `CONNECTING`, — инициатором пары выбирается ровно один
 * участник: сторона с лексикографически меньшим публичным ключом. Вторая
 * только отвечает на входящий `offer`.
 *
 * Логика вынесена отдельно, чтобы покрыть её юнит-тестами без нативного WebRTC.
 */
object PeerConnectionPolicy {

    /** Публичный ключ в hex (нижний регистр, без разделителей). */
    fun keyHex(key: ByteArray): String = key.joinToString("") { "%02x".format(it) }

    /** Мы инициатор пары, если наш ключ строго меньше ключа собеседника. */
    fun weInitiate(myKey: ByteArray, remoteKey: ByteArray): Boolean =
        keyHex(myKey) < keyHex(remoteKey)

    /**
     * Нужно ли (пере)слать `offer` прямо сейчас.
     *
     * @param isInitiator мы инициатор пары
     * @param dataChannelOpen канал уже открыт — слать нечего
     * @param hasExisting у нас уже есть попытка соединения с этим пиром
     * @param ageMs сколько миллисекунд живёт текущая попытка
     * @param retryMs через сколько после создания попытки разрешён повтор
     */
    fun shouldOffer(
        isInitiator: Boolean,
        dataChannelOpen: Boolean,
        hasExisting: Boolean,
        ageMs: Long,
        retryMs: Long,
    ): Boolean {
        if (dataChannelOpen) return false
        if (!isInitiator) return false
        if (hasExisting && ageMs < retryMs) return false
        return true
    }
}

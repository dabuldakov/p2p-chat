package com.p2pchat.app

/** Режимы единственной кнопки отправки. */
internal enum class SendMode { TEXT, VOICE, VIDEO }

internal fun nextSendMode(mode: SendMode): SendMode = when (mode) {
    SendMode.TEXT -> SendMode.VOICE
    SendMode.VOICE -> SendMode.VIDEO
    SendMode.VIDEO -> SendMode.TEXT
}

/** При непустом тексте кнопка всегда работает как отправка текста. */
internal fun effectiveSendMode(input: String, mode: SendMode): SendMode =
    if (input.isBlank()) mode else SendMode.TEXT

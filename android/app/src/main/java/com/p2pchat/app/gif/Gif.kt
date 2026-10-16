package com.p2pchat.app.gif

import kotlinx.serialization.Serializable

/**
 * Гифка в домене приложения.
 *
 * [providerId] — идентификатор у провайдера (id/slug KLIPY). Именно он уходит в
 * сообщении (`Attachment.id`); байты медиа не передаются между клиентами.
 */
@Serializable
data class Gif(
    val providerId: String,
    val title: String? = null,
    /** Анимированный вариант (webp/gif) — фолбэк, если нет mp4. */
    val url: String,
    val previewUrl: String? = null,
    /** mp4 — плавное беззвучное воспроизведение; может отсутствовать. */
    val mp4Url: String? = null,
    val width: Int? = null,
    val height: Int? = null,
)

/** Ссылка из локальной истории (вкладки «Мои»/«Эта группа»): id + размеры. */
data class GifRef(
    val providerId: String,
    val width: Int? = null,
    val height: Int? = null,
)

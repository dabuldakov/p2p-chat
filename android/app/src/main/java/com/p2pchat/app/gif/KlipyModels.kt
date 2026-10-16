package com.p2pchat.app.gif

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * DTO KLIPY GIF API.
 *
 * Поиск/тренды: `{ "result": true, "data": { "data": [ {id, slug, title, file} ],
 * "current_page", "per_page", "has_next" } }`.
 * По id/slug: `{ "result": true, "data": {id, slug, title, file} }`.
 */
@Serializable
data class KlipyGifsResponse(
    val result: Boolean? = null,
    val data: KlipyPage? = null,
) {
    fun items(): List<KlipyGifDto> = data?.items.orEmpty()
}

@Serializable
data class KlipyGifResponse(
    val result: Boolean? = null,
    val data: KlipyGifDto? = null,
)

@Serializable
data class KlipyPage(
    @SerialName("data") val items: List<KlipyGifDto>? = null,
    @SerialName("current_page") val currentPage: Int? = null,
    @SerialName("per_page") val perPage: Int? = null,
    @SerialName("has_next") val hasNext: Boolean? = null,
)

@Serializable
data class KlipyGifDto(
    val id: Long? = null,
    val slug: String? = null,
    val title: String? = null,
    /** Размер → формат (webp/gif/jpg/mp4/webm) → медиа. */
    val file: Map<String, Map<String, KlipyMediaDto>>? = null,
)

@Serializable
data class KlipyMediaDto(
    val url: String? = null,
    val width: Int? = null,
    val height: Int? = null,
    val size: Long? = null,
)

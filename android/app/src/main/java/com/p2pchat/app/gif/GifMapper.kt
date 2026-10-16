package com.p2pchat.app.gif

/**
 * KLIPY DTO → домен.
 *
 * Анимированный вариант: предпочитаем webp (полный цвет, меньше и плавнее),
 * фолбэк — gif. Превью — мелкий статичный jpg/webp для сетки. mp4 — самый
 * плавный/лёгкий вариант. Если нет url или идентификатора — гифка пропускается.
 */
fun KlipyGifDto.toGif(): Gif? {
    val sizes = file ?: return null

    val animated = sizes["hd"]?.get("webp")
        ?: sizes["md"]?.get("webp")
        ?: sizes["sm"]?.get("webp")
        ?: sizes["hd"]?.get("gif")
        ?: sizes["md"]?.get("gif")
        ?: sizes["sm"]?.get("gif")
        ?: sizes.values.firstNotNullOfOrNull { it["webp"] ?: it["gif"] }
    val url = animated?.url ?: return null

    val preview = sizes["sm"]?.get("jpg")?.url
        ?: sizes["sm"]?.get("webp")?.url
        ?: sizes["md"]?.get("jpg")?.url
        ?: sizes["md"]?.get("webp")?.url
        ?: url

    val mp4 = sizes["hd"]?.get("mp4")?.url
        ?: sizes["md"]?.get("mp4")?.url
        ?: sizes["sm"]?.get("mp4")?.url
        ?: sizes.values.firstNotNullOfOrNull { it["mp4"]?.url }

    val providerId = slug ?: id?.toString() ?: return null

    return Gif(
        providerId = providerId,
        title = title,
        url = url,
        previewUrl = preview,
        mp4Url = mp4,
        width = animated.width,
        height = animated.height,
    )
}

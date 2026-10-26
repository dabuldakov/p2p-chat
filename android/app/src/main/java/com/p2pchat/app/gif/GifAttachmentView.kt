package com.p2pchat.app.gif

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.ui.PlayerView
import coil.compose.AsyncImage
import coil.request.ImageRequest
import com.p2pchat.app.GifProvider

/**
 * Показ вложения-гифки в переписке.
 *
 * Резолв `providerId -> Gif` идёт через [GifProvider] (кэш → провайдер). Если
 * есть mp4 — играем через ExoPlayer (петля, без звука, автозапуск) с дисковым
 * кэшем; иначе — анимированный webp/gif через Coil. Плеер освобождается, когда
 * элемент уходит из композиции (например, при прокрутке LazyColumn).
 */
@Composable
fun GifAttachmentView(
    providerId: String,
    width: Int?,
    height: Int?,
    provider: GifProvider,
    modifier: Modifier = Modifier,
) {
    var gif by remember(providerId) { mutableStateOf(provider.cachedGif(providerId)) }
    LaunchedEffect(providerId) {
        if (gif == null) gif = provider.resolveGif(providerId)
    }

    val aspect = remember(width, height) {
        val w = width ?: 0
        val h = height ?: 0
        if (w > 0 && h > 0) w.toFloat() / h.toFloat() else 1f
    }
    val box = modifier
        .widthIn(max = 220.dp)
        .clip(RoundedCornerShape(12.dp))

    val resolved = gif
    when {
        resolved == null -> Box(
            box.aspectRatio(aspect).background(Color(0x22000000)),
            contentAlignment = Alignment.Center,
        ) { CircularProgressIndicator(strokeWidth = 2.dp) }

        !resolved.mp4Url.isNullOrBlank() -> GifVideoPlayer(resolved.mp4Url, box.aspectRatio(aspect))

        else -> AsyncImage(
            model = ImageRequest.Builder(LocalContext.current).data(resolved.url).crossfade(true).build(),
            contentDescription = resolved.title,
            contentScale = ContentScale.Fit,
            modifier = box.aspectRatio(aspect),
        )
    }
}

@androidx.annotation.OptIn(androidx.media3.common.util.UnstableApi::class)
@Composable
private fun GifVideoPlayer(url: String, modifier: Modifier) {
    val context = LocalContext.current
    val player = remember(url) {
        ExoPlayer.Builder(context)
            .setMediaSourceFactory(DefaultMediaSourceFactory(Media3Cache.dataSourceFactory()))
            .build()
            .apply {
                repeatMode = Player.REPEAT_MODE_ONE
                volume = 0f
                setMediaItem(MediaItem.fromUri(url))
                prepare()
                playWhenReady = true
            }
    }
    DisposableEffect(url) {
        onDispose { player.release() }
    }
    AndroidView(
        factory = { ctx ->
            PlayerView(ctx).apply {
                useController = false
                this.player = player
            }
        },
        modifier = modifier,
    )
}

package com.p2pchat.app

import android.graphics.BitmapFactory
import android.media.MediaPlayer
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.TextButton
import androidx.compose.material3.Text
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.media3.common.MediaItem
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.ui.PlayerView
import coil.compose.AsyncImage
import uniffi.p2pchat_ffi.FfiAttachment
import java.io.File

@Composable
fun MediaAttachmentView(att: FfiAttachment, onLongPress: () -> Unit, onRetry: () -> Unit) {
    val ctx = LocalContext.current
    val file = remember(att.id, att.mime) { MediaFiles.file(ctx, att.id, att.mime) }
    val exists = file.isFile && file.length() > 0
    when (att.kind) {
        "image" -> ImageAttachment(att, file, exists, onLongPress, onRetry)
        "video", "video_note" -> VideoAttachment(att, file, exists, onLongPress, onRetry)
        "audio", "voice" -> AudioAttachment(att, file, exists, onRetry)
    }
}

@Composable
private fun ImageAttachment(
    att: FfiAttachment,
    file: File,
    exists: Boolean,
    onLongPress: () -> Unit,
    onRetry: () -> Unit,
) {
    var full by remember { mutableStateOf(false) }
    val thumb = remember(att.thumbnail) { thumbBitmap(att.thumbnail) }
    Box(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 120.dp, max = 280.dp)
            .clip(RoundedCornerShape(10.dp))
            .clickable(enabled = exists) { full = true }
            .then(if (exists) Modifier else Modifier),
        contentAlignment = Alignment.Center,
    ) {
        if (exists) {
            AsyncImage(
                model = file,
                contentDescription = att.fileName,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp, max = 280.dp),
            )
        } else if (thumb != null) {
            Image(
                bitmap = thumb.asImageBitmap(),
                contentDescription = att.fileName,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp, max = 280.dp),
            )
            RetryOverlay(onRetry)
        } else {
            Box(Modifier.fillMaxWidth().height(160.dp).background(MaterialTheme.colorScheme.surfaceVariant))
            RetryOverlay(onRetry)
        }
    }
    if (full && exists) {
        Dialog(onDismissRequest = { full = false }) {
            Surface(shape = RoundedCornerShape(12.dp)) {
                AsyncImage(
                    model = file,
                    contentDescription = att.fileName,
                    contentScale = ContentScale.Fit,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
        }
    }
    // Долгий тап по превью тоже открывает действия сообщения.
    if (!exists) {
        Box(Modifier.fillMaxWidth().height(1.dp).clickable(onClick = onLongPress))
    }
}

@Composable
private fun VideoAttachment(
    att: FfiAttachment,
    file: File,
    exists: Boolean,
    onLongPress: () -> Unit,
    onRetry: () -> Unit,
) {
    var play by remember { mutableStateOf(false) }
    val thumb = remember(att.thumbnail) { thumbBitmap(att.thumbnail) }
    Box(
        Modifier
            .fillMaxWidth()
            .heightIn(min = 140.dp, max = 260.dp)
            .clip(RoundedCornerShape(12.dp))
            .clickable(enabled = exists) { play = true },
        contentAlignment = Alignment.Center,
    ) {
        if (thumb != null) {
            Image(
                bitmap = thumb.asImageBitmap(),
                contentDescription = att.fileName,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxWidth().heightIn(min = 140.dp, max = 260.dp),
            )
        } else {
            Box(Modifier.fillMaxWidth().height(180.dp).background(MaterialTheme.colorScheme.surfaceVariant))
        }
        if (exists) {
            Surface(
                shape = RoundedCornerShape(50),
                color = MaterialTheme.colorScheme.surface.copy(alpha = 0.85f),
            ) {
                Icon(
                    Icons.Filled.PlayArrow,
                    contentDescription = "Play",
                    modifier = Modifier.padding(10.dp).size(32.dp),
                )
            }
        } else {
            RetryOverlay(onRetry)
        }
    }
    if (play && exists) {
        VideoPlayerDialog(file) { play = false }
    }
    if (!exists) {
        Box(Modifier.fillMaxWidth().height(1.dp).clickable(onClick = onLongPress))
    }
}

@Composable
private fun VideoPlayerDialog(file: File, onDismiss: () -> Unit) {
    val ctx = LocalContext.current
    val player = remember(file) {
        ExoPlayer.Builder(ctx).build().apply {
            setMediaItem(MediaItem.fromUri(file.toURI().toString()))
            prepare()
            playWhenReady = true
        }
    }
    DisposableEffect(player) { onDispose { player.release() } }
    Dialog(onDismissRequest = onDismiss) {
        Surface(shape = RoundedCornerShape(14.dp)) {
            AndroidView(
                factory = { c ->
                    PlayerView(c).apply {
                        this.player = player
                        useController = true
                    }
                },
                modifier = Modifier.fillMaxWidth().height(320.dp),
            )
        }
    }
}

@Composable
private fun AudioAttachment(att: FfiAttachment, file: File, exists: Boolean, onRetry: () -> Unit) {
    var playing by remember { mutableStateOf(false) }
    var player by remember { mutableStateOf<MediaPlayer?>(null) }
    DisposableEffect(file) {
        onDispose {
            runCatching { player?.release() }
            player = null
        }
    }
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .background(MaterialTheme.colorScheme.surfaceVariant)
            .padding(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(
            enabled = exists,
            onClick = {
                if (playing) {
                    runCatching { player?.pause() }
                    playing = false
                } else {
                    val mp = player ?: runCatching {
                        MediaPlayer().apply {
                            setDataSource(file.absolutePath)
                            prepare()
                            setOnCompletionListener { playing = false }
                        }
                    }.getOrNull()?.also { player = it }
                    mp?.start()
                    playing = true
                }
            },
        ) {
            Icon(
                if (playing) Icons.Filled.Pause else Icons.Filled.PlayArrow,
                contentDescription = if (playing) "Pause" else "Play",
            )
        }
        Column(Modifier.weight(1f)) {
            Text(
                if (att.kind == "voice") stringResource(R.string.voice_message)
                else att.fileName.ifBlank { stringResource(R.string.attachment_audio) },
                style = MaterialTheme.typography.bodySmall,
            )
            Text(
                if (att.durationMs > 0uL) fmtDuration(att.durationMs.toLong()) else "—",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (!exists) RetryOverlay(onRetry)
    }
}

@Composable
private fun RetryOverlay(onRetry: () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        CircularProgressIndicator(Modifier.size(24.dp))
        TextButton(onClick = onRetry) { Text(stringResource(R.string.download)) }
    }
}

private fun thumbBitmap(bytes: ByteArray) =
    if (bytes.isEmpty()) null else runCatching { BitmapFactory.decodeByteArray(bytes, 0, bytes.size) }.getOrNull()

private fun fmtDuration(ms: Long): String {
    val sec = ms / 1000
    return "%d:%02d".format(sec / 60, sec % 60)
}

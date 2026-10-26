package com.p2pchat.app

import androidx.core.graphics.scale
import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.provider.MediaStore
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.Process
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.core.content.ContextCompat
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Canvas
import coil.compose.AsyncImage
import coil.request.ImageRequest
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AdminPanelSettings
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Gavel
import androidx.compose.material.icons.filled.GifBox
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.Mic
import androidx.compose.material.icons.filled.PersonAdd
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.QrCode
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.HowToVote
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.Storage
import androidx.compose.material.icons.filled.Videocam
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.unit.Dp
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.flow.collect
import androidx.compose.ui.Alignment
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.withContext
import kotlin.math.absoluteValue
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.p2pchat.app.gif.GifAttachmentView
import com.p2pchat.app.gif.GifPickerDialog
import uniffi.p2pchat_ffi.FfiMessage

/** Кликабельная метка типа соединения (STUN/TURN). */
@Composable
internal fun ConnTypeChip(label: String, active: Boolean, color: Color, onClick: () -> Unit) {
    Text(
        label,
        style = MaterialTheme.typography.titleSmall,
        color = if (active) color else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.35f),
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 4.dp),
    )
}

/** Краткая схема и описание выбранного типа соединения. */
@Composable
internal fun ConnExplanation(kind: String) {
    val isStun = kind == "stun"
    val title = if (isStun) stringResource(R.string.conn_stun_title) else stringResource(R.string.conn_turn_title)
    val scheme = if (isStun) stringResource(R.string.conn_stun_scheme) else stringResource(R.string.conn_turn_scheme)
    val desc = if (isStun) {
        stringResource(R.string.conn_stun_desc)
    } else {
        stringResource(R.string.conn_turn_desc)
    }
    Surface(
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(10.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            Text(
                scheme,
                style = MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace),
                color = MaterialTheme.colorScheme.primary,
            )
            Text(desc, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
internal fun Avatar(bytes: ByteArray?, tag: String, size: Dp = 40.dp) {
    val bg = remember(tag) { Color.hsl((tag.hashCode().absoluteValue % 360).toFloat(), 0.45f, 0.5f) }
    Box(
        Modifier.size(size).clip(RoundedCornerShape(size * 0.28f)).background(bg),
        contentAlignment = Alignment.Center,
    ) {
        if (bytes != null && bytes.isNotEmpty()) {
            val ctx = LocalContext.current
            val key = "av:${tag}:${bytes.size}:${bytes.contentHashCode()}"
            AsyncImage(
                model = ImageRequest.Builder(ctx).data(bytes).memoryCacheKey(key).crossfade(false).build(),
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
        } else {
            Text(tag.take(1).uppercase(), color = Color.White, style = MaterialTheme.typography.labelLarge)
        }
    }
}

/** Прочитать картинку из Uri, уменьшить до [max] и сжать в JPEG. */
internal fun downscaledJpeg(ctx: Context, uri: Uri, max: Int = 128): ByteArray? = runCatching {
    val src = ctx.contentResolver.openInputStream(uri)?.use {
        android.graphics.BitmapFactory.decodeStream(it)
    } ?: return null
    val scale = (max.toFloat() / maxOf(src.width, src.height)).coerceAtMost(1f)
    val w = (src.width * scale).toInt().coerceAtLeast(1)
    val h = (src.height * scale).toInt().coerceAtLeast(1)
    val bmp = if (scale < 1f) src.scale(w, h) else src
    val bos = java.io.ByteArrayOutputStream()
    bmp.compress(android.graphics.Bitmap.CompressFormat.JPEG, 80, bos)
    bos.toByteArray()
}.getOrNull()

/** Время последней активности сигналинга в виде «HH:mm:ss» (или «—»). */
internal fun fmtTime(ts: Long): String {
    if (ts <= 0L) return "—"
    return java.text.SimpleDateFormat("HH:mm:ss", java.util.Locale.getDefault())
        .format(java.util.Date(ts))
}

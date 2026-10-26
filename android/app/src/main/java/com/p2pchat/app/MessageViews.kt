package com.p2pchat.app

import androidx.core.net.toUri
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

internal fun ByteArray.toHexShort(): String =
    joinToString("") { "%02x".format(it) }.take(6)

private const val URL_TAG = "url"

/** Время сообщения с плавным появлением/исчезновением. */
@Composable
private fun MessageTime(text: String, visible: Boolean) {
    AnimatedVisibility(
        visible = visible,
        enter = fadeIn(tween(300)),
        exit = fadeOut(tween(300)),
    ) {
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 4.dp),
        )
    }
}

/** Пузырь сообщения: владелец — справа, собеседник — слева. */
@Composable
@OptIn(ExperimentalFoundationApi::class)
internal fun MessageRow(
    mine: Boolean,
    name: String,
    message: FfiMessage,
    vm: ChatViewModel,
    onTap: () -> Unit,
    onLongPress: () -> Unit,
    onOpenProfile: (String) -> Unit,
    showTime: Boolean,
) {
    val ctx = LocalContext.current
    val timeFmt = remember { android.text.format.DateFormat.getTimeFormat(ctx) }
    val timeText = remember(message.wallTime) {
        timeFmt.format(java.util.Date(message.wallTime.toLong() * 1000))
    }

    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
        horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (!mine) {
            val ahex = message.author.toHex()
            Box(
                Modifier.size(36.dp).clickable { onOpenProfile(ahex) },
                contentAlignment = Alignment.Center,
            ) {
                Avatar(vm.avatars[ahex], ahex, 28.dp)
            }
            Spacer(Modifier.width(4.dp))
        }
        if (mine) {
            MessageTime(timeText, showTime)
        }
        if (mine && message.edited) {
            Icon(
                Icons.Filled.Edit,
                contentDescription = stringResource(R.string.edit_message),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(13.dp),
            )
            Spacer(Modifier.width(4.dp))
        }
        Surface(
            color = if (mine) MaterialTheme.colorScheme.primaryContainer
            else MaterialTheme.colorScheme.surface,
            shape = RoundedCornerShape(12.dp),
            tonalElevation = 1.dp,
            modifier = Modifier
                .widthIn(max = 260.dp)
                .combinedClickable(onClick = onTap, onLongClick = onLongPress),
        ) {
            Column(Modifier.padding(8.dp)) {
                if (!mine) {
                    Text(
                        name,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                val att = message.attachment
                if (message.deleted) {
                    Text(
                        stringResource(R.string.message_deleted),
                        style = MaterialTheme.typography.bodyMedium,
                        fontStyle = FontStyle.Italic,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else if (att != null) {
                    if (att.kind in setOf("image", "video", "video_note", "audio", "voice")) {
                        MediaAttachmentView(att, onLongPress) { vm.retryMedia(att.id, att.mime) }
                    } else {
                        GifAttachmentView(
                            providerId = att.id,
                            width = att.width.toInt(),
                            height = att.height.toInt(),
                            vm = vm,
                        )
                    }
                } else {
                    val inv = extractInvite(message.text)
                    val url = singleUrl(message.text)
                    when {
                        inv != null -> InviteCard(inv.first, inv.second, vm, onLongPress)
                        url != null ->
                            LinkPreviewCard(url, onLongPress = onLongPress) {
                                MessageText(message.text, onLongPress, onTap)
                            }
                        else -> MessageText(message.text, onLongPress, onTap)
                    }
                }
                if (!message.deleted && message.reactions.isNotEmpty()) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .horizontalScroll(rememberScrollState())
                            .padding(top = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        message.reactions.forEach { r ->
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = if (r.mine) MaterialTheme.colorScheme.primaryContainer
                                else MaterialTheme.colorScheme.surfaceVariant,
                                modifier = Modifier.clip(RoundedCornerShape(12.dp)).clickable {
                                    vm.react(message.id, r.emoji, !r.mine)
                                },
                            ) {
                                Text(
                                    "${r.emoji} ${r.count}",
                                    style = MaterialTheme.typography.labelMedium,
                                    modifier = Modifier.padding(horizontal = 7.dp, vertical = 3.dp),
                                )
                            }
                        }
                    }
                }
            }
        }
        if (!mine) {
            MessageTime(timeText, showTime)
        }
        if (!mine && message.edited) {
            Spacer(Modifier.width(4.dp))
            Icon(
                Icons.Filled.Edit,
                contentDescription = stringResource(R.string.edit_message),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.size(13.dp),
            )
        }
    }
}

/** Текст сообщения: тап по ссылке открывает её, долгий тап — редактирование. */
@Composable
private fun MessageText(
    text: String,
    onLongPress: () -> Unit = {},
    onTap: () -> Unit = {},
) {
    val ctx = LocalContext.current
    val annotated = remember(text) { linksAnnotated(text) }
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    Text(
        text = annotated,
        style = MaterialTheme.typography.bodyMedium.copy(color = MaterialTheme.colorScheme.onSurface),
        onTextLayout = { layout = it },
        modifier = Modifier.pointerInput(text) {
            detectTapGestures(
                onLongPress = { onLongPress() },
                onTap = { pos ->
                    val l = layout ?: return@detectTapGestures
                    val offset = l.getOffsetForPosition(pos)
                    val ann = annotated.getStringAnnotations(URL_TAG, offset, offset).firstOrNull()
                    if (ann != null) {
                        runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, ann.item.toUri())) }
                    } else {
                        onTap()
                    }
                },
            )
        },
    )
}

/**
 * Если сообщение целиком содержит одну ссылку-инвайт `p2pchat://join?…`,
 * вернуть (ссылку, имя группы). Имя группы — это group_id (`g`), т.к. он
 * совпадает с именем при создании группы.
 */
internal fun extractInvite(text: String): Pair<String, String>? {
    val t = text.trim()
    val idx = t.indexOf("p2pchat://join?")
    if (idx < 0) return null
    var end = idx
    while (end < t.length && !t[end].isWhitespace()) end++
    val link = t.substring(idx, end)
    if (t.removeRange(idx, end).isNotBlank()) return null
    val query = link.substringAfter('?', "").split('&')
    fun param(k: String) = query.firstOrNull { it.startsWith("$k=") }?.substringAfter("=")
    val gHex = param("g") ?: return null
    // Имя — из подписанного параметра `m`; для старых билетов — из `g` (id == имя).
    val name = param("m")?.takeIf { it.isNotEmpty() }
        ?.let { hexToBytes(it)?.toString(Charsets.UTF_8) }
        ?: if (gHex.length % 2 == 0) hexToBytes(gHex)?.toString(Charsets.UTF_8) else null
    if (name.isNullOrBlank()) return null
    return link to name
}

/** Если всё сообщение — одна ссылка (без текста), вернуть её; иначе null. */
internal fun singleUrl(text: String): String? {
    val t = text.trim()
    val spans = urlSpans(t)
    return if (spans.size == 1 && spans[0].start == 0 && spans[0].end == t.length) spans[0].url else null
}

private fun linksAnnotated(text: String): AnnotatedString = buildAnnotatedString {
    var last = 0
    for (span in urlSpans(text)) {
        append(text.substring(last, span.start))
        pushStringAnnotation(URL_TAG, span.url)
        withStyle(SpanStyle(color = Color(0xFF5B8CFF), textDecoration = TextDecoration.Underline)) {
            append(span.url)
        }
        pop()
        last = span.end
    }
    append(text.substring(last))
}

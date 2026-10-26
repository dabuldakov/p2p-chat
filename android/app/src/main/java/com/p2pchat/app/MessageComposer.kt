package com.p2pchat.app

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

@Composable
internal fun MessageComposer(
    gifEnabled: Boolean,
    sharedDraft: String?,
    onConsumeSharedDraft: () -> Unit,
    onSend: (String) -> Unit,
    onSendMedia: (String, PreparedMedia) -> Unit,
    gifPicker: @Composable (onDismiss: () -> Unit) -> Unit,
) {
    var input by remember { mutableStateOf("") }
    var showGifPicker by remember { mutableStateOf(false) }
    var showAttachMenu by remember { mutableStateOf(false) }
    var mediaBusy by remember { mutableStateOf(false) }
    var recording by remember { mutableStateOf(false) }
    var recordSeconds by remember { mutableStateOf(0) }
    var sendMode by remember { mutableStateOf(SendMode.TEXT) }
    val ctx = LocalContext.current
    val recorder = remember { AudioRecorder(ctx) }
    val scope = rememberCoroutineScope()

    fun sendPicked(uri: android.net.Uri, kind: String, prep: suspend (android.net.Uri) -> PreparedMedia?) {
        scope.launch {
            mediaBusy = true
            val p = prep(uri)
            mediaBusy = false
            if (p == null) Toast.makeText(ctx, ctx.getString(R.string.media_failed), Toast.LENGTH_LONG).show()
            else onSendMedia(kind, p)
        }
    }

    val pickPhoto = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        uri?.let { sendPicked(it, "image") { u -> MediaPrep.image(ctx, u) } }
    }
    val pickVideo = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { uri ->
        uri?.let { sendPicked(it, "video") { u -> MediaPrep.video(ctx, u) } }
    }
    val pickAudio = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { sendPicked(it, "audio") { u -> MediaPrep.audio(ctx, u) } }
    }
    val captureVideo = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { res ->
        res.data?.data?.let { uri ->
            sendPicked(uri, "video_note") { u -> MediaPrep.video(ctx, u) }
        }
    }

    fun stopVoice(send: Boolean) {
        val file = recorder.stop()
        recording = false
        sendMode = SendMode.TEXT
        if (!send) {
            file?.delete()
            return
        }
        if (file == null) {
            Toast.makeText(ctx, ctx.getString(R.string.media_failed), Toast.LENGTH_LONG).show()
            return
        }
        scope.launch {
            mediaBusy = true
            val p = MediaPrep.recorded(ctx, file, "voice", "audio/mp4")
            mediaBusy = false
            if (p == null) Toast.makeText(ctx, ctx.getString(R.string.media_failed), Toast.LENGTH_LONG).show()
            else onSendMedia("voice", p)
        }
    }

    val micPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (!granted) Toast.makeText(ctx, ctx.getString(R.string.mic_permission), Toast.LENGTH_LONG).show()
    }

    fun startVoice(): Boolean {
        if (ContextCompat.checkSelfPermission(ctx, Manifest.permission.RECORD_AUDIO) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            micPermission.launch(Manifest.permission.RECORD_AUDIO)
            return false
        }
        if (!recorder.start()) {
            Toast.makeText(ctx, ctx.getString(R.string.media_failed), Toast.LENGTH_LONG).show()
            return false
        }
        recordSeconds = 0
        recording = true
        return true
    }

    fun cycleSendMode() {
        if (input.isNotBlank()) {
            onSend(input)
            input = ""
            sendMode = SendMode.TEXT
            return
        }
        sendMode = nextSendMode(sendMode)
    }

    LaunchedEffect(recording) {
        while (recording) {
            kotlinx.coroutines.delay(1000)
            if (recording) {
                recordSeconds += 1
                if (recordSeconds >= 300) stopVoice(true)
            }
        }
    }
    DisposableEffect(Unit) { onDispose { recorder.cancel() } }

    // Черновик из «Поделиться»: один раз подставляем в поле ввода.
    val shared = sharedDraft
    LaunchedEffect(shared) {
        if (shared != null) {
            input = shared
            onConsumeSharedDraft()
        }
    }

        Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.Bottom) {
            IconButton(onClick = { showAttachMenu = true }, enabled = !mediaBusy && !recording) {
                Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.attachment))
            }
            if (gifEnabled) {
                IconButton(onClick = { showGifPicker = true }, enabled = !mediaBusy && !recording) {
                    Icon(Icons.Filled.GifBox, contentDescription = stringResource(R.string.gif))
                }
            }
            OutlinedTextField(
                value = if (recording) "%d:%02d".format(recordSeconds / 60, recordSeconds % 60) else input,
                onValueChange = {
                    if (!recording) {
                        input = it
                        if (it.isNotBlank()) sendMode = SendMode.TEXT
                    }
                },
                modifier = Modifier.weight(1f),
                enabled = !recording,
                placeholder = { Text(stringResource(R.string.message_hint)) },
                minLines = 1,
                maxLines = 5,
                shape = RoundedCornerShape(12.dp),
                leadingIcon = if (recording) {
                    {
                        Box(Modifier.size(10.dp).clip(RoundedCornerShape(50)).background(Color(0xFFE53935)))
                    }
                } else null,
                textStyle = MaterialTheme.typography.bodyMedium.copy(
                    color = MaterialTheme.colorScheme.onSurface,
                ),
            )
            if (mediaBusy) {
                CircularProgressIndicator(Modifier.size(24.dp))
                Spacer(Modifier.width(8.dp))
            }
            Spacer(Modifier.width(8.dp))
            val mode = effectiveSendMode(input, sendMode)
            Box(
                modifier = Modifier
                    .size(48.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(MaterialTheme.colorScheme.primary)
                    .pointerInput(input, sendMode, mediaBusy) {
                        if (mediaBusy) return@pointerInput
                        awaitEachGesture {
                            awaitFirstDown()
                            val releasedEarly = withTimeoutOrNull(350L) {
                                waitForUpOrCancellation() != null
                            }
                            if (releasedEarly == true) {
                                cycleSendMode()
                                return@awaitEachGesture
                            }
                            // Удержание.
                            when {
                                input.isNotBlank() -> Unit
                                mode == SendMode.VOICE -> {
                                    if (startVoice()) {
                                        val released = waitForUpOrCancellation() != null
                                        stopVoice(released)
                                    }
                                }
                                mode == SendMode.VIDEO -> {
                                    captureVideo.launch(
                                        Intent(MediaStore.ACTION_VIDEO_CAPTURE)
                                            .putExtra(MediaStore.EXTRA_DURATION_LIMIT, 60),
                                    )
                                    waitForUpOrCancellation()
                                    sendMode = SendMode.TEXT
                                }
                            }
                        }
                    },
                contentAlignment = Alignment.Center,
            ) {
                if (mode == SendMode.TEXT) {
                    Icon(
                        painter = painterResource(R.drawable.ic_send_logo),
                        contentDescription = stringResource(R.string.send),
                        tint = MaterialTheme.colorScheme.onPrimary,
                    )
                } else {
                    Icon(
                        if (mode == SendMode.VOICE) Icons.Filled.Mic else Icons.Filled.Videocam,
                        contentDescription = stringResource(
                            if (mode == SendMode.VOICE) R.string.voice_message else R.string.video_message,
                        ),
                        tint = MaterialTheme.colorScheme.onPrimary,
                    )
                }
            }
        }

    if (showAttachMenu) {
        AlertDialog(
            onDismissRequest = { showAttachMenu = false },
            title = { Text(stringResource(R.string.attachment)) },
            text = {
                Column {
                    TextButton(onClick = {
                        showAttachMenu = false
                        pickPhoto.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly))
                    }) { Text(stringResource(R.string.attachment_photo)) }
                    TextButton(onClick = {
                        showAttachMenu = false
                        pickVideo.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.VideoOnly))
                    }) { Text(stringResource(R.string.attachment_video)) }
                    TextButton(onClick = {
                        showAttachMenu = false
                        pickAudio.launch(arrayOf("audio/*"))
                    }) { Text(stringResource(R.string.attachment_audio)) }
                }
            },
            confirmButton = {},
            dismissButton = {
                TextButton(onClick = { showAttachMenu = false }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }

    if (showGifPicker && gifEnabled) {
        gifPicker { showGifPicker = false }
    }

}

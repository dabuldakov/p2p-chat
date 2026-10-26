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

@Composable
internal fun SettingsScreen(vm: ChatViewModel) {
    val state by vm.uiState.collectAsState()
    val dialogs = rememberSettingsDialogsState()
    val ctx = LocalContext.current
    val update = state.update
    var nick by remember(state.myNickname) { mutableStateOf(state.myNickname ?: "") }
    var editingNick by remember(state.myNickname) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val pickAvatar = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        uri?.let { u ->
            scope.launch {
                withContext(Dispatchers.IO) { downscaledJpeg(ctx, u) }?.let { vm.setAvatar(it) }
            }
        }
    }
    var explainKind by remember { mutableStateOf<String?>(null) }
    val storageTotal by produceState<Long?>(null, dialogs.showStorage) {
        value = withContext(Dispatchers.IO) { vm.storageStats().sumOf { it.size } }
    }
    val fingerprint = remember { vm.fingerprint() }

    fun openDoc(page: String) {
        val path = if (Lang.get(ctx) == "ru") "ru/$page" else page
        runCatching {
            ctx.startActivity(
                Intent(
                    Intent.ACTION_VIEW,
                    ("https://dabuldakov.github.io/p2p-chat/" + path).toUri(),
                ),
            )
        }
    }

    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        SettingsVersionCard(
            version = state.version,
            update = update,
            updateCheckFailed = state.updateCheckFailed,
            onClick = {
                if (update != null) {
                    dialogs.showUpdate = true
                } else {
                    vm.checkNow { has ->
                        if (has) dialogs.showUpdate = true
                        else Toast.makeText(ctx, ctx.getString(R.string.up_to_date), Toast.LENGTH_SHORT).show()
                    }
                }
            },
        )

        SettingsStorageCard(
            sizeText = storageTotal?.let(::formatBytes) ?: "…",
            onClick = { dialogs.showStorage = true },
        )

        SettingsNicknameCard(
            nickname = state.myNickname ?: "",
            editing = editingNick,
            draft = nick,
            onDraftChange = { nick = it },
            onEdit = {
                nick = state.myNickname ?: ""
                editingNick = true
            },
            onSave = {
                vm.setNickname(nick)
                editingNick = false
            },
        )

        SettingsAvatarCard(
            avatar = state.avatars[state.myPeerId],
            label = state.myNickname ?: state.myPeerId.take(8),
            peerId = state.myPeerId,
            onChange = { pickAvatar.launch("image/*") },
        )

        SettingsBackupCard(
            fingerprint = fingerprint,
            onShowSeed = { dialogs.showSeedConfirm = true },
            onRestore = {
                if (state.groupInfos.isNotEmpty()) {
                    Toast.makeText(ctx, ctx.getString(R.string.restore_blocked), Toast.LENGTH_LONG).show()
                } else {
                    dialogs.restoreText = ""
                    dialogs.showRestore = true
                }
            },
        )

        SettingsLinkCard(stringResource(R.string.terms)) { openDoc("terms.html") }

        if (state.blocked.isNotEmpty()) {
            SettingsBlockedCard(
                blocked = state.blocked,
                nicknames = state.nicknames,
                onUnblock = { hex -> hexToBytes(hex)?.let { vm.unblockPeer(it) } },
            )
        }

        SettingsLinkCard(stringResource(R.string.privacy_policy)) { openDoc("privacy.html") }

        SettingsToggleCard(
            title = stringResource(R.string.keep_alive_title),
            description = stringResource(R.string.keep_alive_desc),
            checked = state.backgroundEnabled,
            onChange = vm::setBackground,
        )

        SettingsToggleCard(
            title = stringResource(R.string.debug_title),
            description = stringResource(R.string.debug_desc),
            checked = state.debugLogsEnabled,
            onChange = vm::setDebugLogs,
        )

        val lang = Lang.LANGUAGES.firstOrNull { it.first == Lang.get(ctx) } ?: Lang.LANGUAGES.first()
        SettingsLanguageCard("${lang.second}  ${lang.third}") { dialogs.showLang = true }

        SettingsConnectionCard(
            direct = state.directCount,
            turn = state.turnCount,
            lastSignaling = state.lastSignalingTs,
            explainKind = explainKind,
            onExplain = { kind -> explainKind = if (explainKind == kind) null else kind },
        )

        SettingsGraphCard { dialogs.showGraphAll = true }
    }

    SettingsDialogs(vm, dialogs, update)
}

package com.p2pchat.app

import androidx.compose.ui.res.pluralStringResource
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
internal fun GroupHeader(vm: ChatViewModel, state: ChatUiState, dialogs: GroupDialogsState) {
    var inviteExpanded by remember { mutableStateOf(false) }
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val pickGroupAvatar = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        uri?.let { u ->
            scope.launch {
                withContext(Dispatchers.IO) { downscaledJpeg(ctx, u) }?.let { vm.setGroupAvatar(it) }
            }
        }
    }
    val nowSec = (System.currentTimeMillis() / 1000L).toULong()
    val ownerAbsentDays = if (!state.isOwner && state.ownerLastSeen > 0L) {
        ((System.currentTimeMillis() - state.ownerLastSeen) / 86_400_000L).toInt()
    } else {
        0
    }
    val canRequest = !state.isOwner && ownerAbsentDays >= 7
    val canChallenge = !state.isOwner && (
        state.ownerChallengeAt == 0uL || nowSec >= state.ownerChallengeAt + 7uL * 86_400uL
        )
    val electionActive = state.ownerElection != null
    val showOwnerActions = ownerActionsVisible(state.isOwner)
    val requestEnabled = ownerActionEnabled(canRequest, electionActive)
    val challengeEnabled = ownerActionEnabled(canChallenge, electionActive)
    val showOwnerHint = ownerManagementHintVisible(state.isOwner, inviteExpanded)

    // Свёрнутый по умолчанию блок приглашения
    Card {
        Row(
            modifier = Modifier.fillMaxWidth().clickable { inviteExpanded = !inviteExpanded }.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val gname = state.currentGroupName ?: ""
            Box(
                Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .then(if (state.isOwner) Modifier.clickable { pickGroupAvatar.launch("image/*") } else Modifier),
            ) {
                Avatar(state.groupAvatar, gname, 32.dp)
            }
            Spacer(Modifier.width(10.dp))
            Text(
                if (state.isOwner) stringResource(R.string.invite_to_group)
                else (state.currentGroupName ?: ""),
                style = MaterialTheme.typography.titleSmall,
            )
            Spacer(Modifier.weight(1f))
            Text(pluralStringResource(R.plurals.members_count, state.members.size, state.members.size), style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (showOwnerHint) {
                IconButton(onClick = { inviteExpanded = true }) {
                    Icon(
                        Icons.Filled.AdminPanelSettings,
                        contentDescription = stringResource(R.string.owner_management),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            if (state.isOwner) {
                IconButton(onClick = { dialogs.renameText = state.currentGroupName ?: ""; dialogs.showRename = true }) {
                    Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.rename_group))
                }
            }
            Spacer(Modifier.width(8.dp))
            Icon(
                if (inviteExpanded) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown,
                contentDescription = stringResource(R.string.expand),
            )
        }
        if (inviteExpanded) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, bottom = 16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (state.isOwner) {
                        IconButton(onClick = { dialogs.qrShown = true }) {
                            Icon(Icons.Filled.QrCode, contentDescription = stringResource(R.string.invite_qr))
                        }
                    }
                    IconButton(onClick = { dialogs.showGraph = true }) {
                        Icon(Icons.Filled.Hub, contentDescription = stringResource(R.string.connection))
                    }
                }
                if (state.isOwner) {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = {
                            vm.regenInvite(1) { link ->
                                link?.let {
                                    clipboard.setText(AnnotatedString(it))
                                    Toast.makeText(ctx, ctx.getString(R.string.invite_copied), Toast.LENGTH_SHORT).show()
                                }
                            }
                        }) { Text(stringResource(R.string.invite_single)) }
                        OutlinedButton(onClick = {
                            vm.regenInvite(5) { link ->
                                link?.let {
                                    clipboard.setText(AnnotatedString(it))
                                    Toast.makeText(ctx, ctx.getString(R.string.invite_copied), Toast.LENGTH_SHORT).show()
                                }
                            }
                        }) { Text(stringResource(R.string.invite_5)) }
                    }
                }
                Text(
                    stringResource(
                        R.string.status_connected,
                        stringResource(R.string.neighbors, state.groupNeighbors),
                        state.groupConnected,
                    ),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (state.status.isNotBlank()) {
                    Text(
                        state.status,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Column(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(max = 320.dp)
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    Text(stringResource(R.string.participants), style = MaterialTheme.typography.titleSmall)
                    state.members.forEach { m ->
                        val hex = m.peerId.toHex()
                        Row(
                            Modifier.fillMaxWidth().clickable { dialogs.profileTarget = hex },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            val label = (state.nicknames[hex]?.takeIf { it.isNotBlank() }
                                ?: hex.take(10)) + (if (hex == state.myPeerId) stringResource(R.string.you) else "")
                            Avatar(state.avatars[hex], hex, 26.dp)
                            Spacer(Modifier.width(8.dp))
                            Text(label, style = MaterialTheme.typography.bodySmall)
                            if (hex == state.ownerId) {
                                Spacer(Modifier.width(4.dp))
                                Icon(
                                    Icons.Filled.Star,
                                    contentDescription = stringResource(R.string.owner_badge),
                                    tint = Color(0xFF5B8CFF),
                                    modifier = Modifier.size(16.dp),
                                )
                            }
                            Spacer(Modifier.weight(1f))
                            val conn = state.peerKinds[m.publicKey.toHex()]
                            if (conn == "turn" || conn == "direct") {
                                Text(
                                    if (conn == "turn") "TURN" else "STUN",
                                    style = MaterialTheme.typography.labelSmall,
                                    color = if (conn == "turn") Color(0xFFE08A1E) else Color(0xFF2E9E6B),
                                )
                                Spacer(Modifier.width(8.dp))
                            }
                            if (state.isOwner && hex != state.myPeerId) {
                                IconButton(onClick = { dialogs.removeTarget = m.peerId }) {
                                    Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.remove))
                                }
                            }
                        }
                    }
                }
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (showOwnerActions) {
                        IconButton(
                            onClick = {
                                when {
                                    requestEnabled -> vm.claimOwner(0)
                                    electionActive -> Toast.makeText(
                                        ctx,
                                        ctx.getString(R.string.owner_election_in_progress),
                                        Toast.LENGTH_LONG,
                                    ).show()
                                    else -> dialogs.showRequestInfo = true
                                }
                            },
                            modifier = Modifier.alpha(if (requestEnabled) 1f else 0.45f),
                        ) {
                            Icon(
                                Icons.Filled.PersonAdd,
                                contentDescription = stringResource(R.string.owner_request),
                                tint = if (requestEnabled) MaterialTheme.colorScheme.onSurfaceVariant
                                else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.45f),
                            )
                        }
                        IconButton(
                            onClick = {
                                when {
                                    challengeEnabled -> vm.claimOwner(1)
                                    electionActive -> Toast.makeText(
                                        ctx,
                                        ctx.getString(R.string.owner_election_in_progress),
                                        Toast.LENGTH_LONG,
                                    ).show()
                                    else -> Toast.makeText(
                                        ctx,
                                        ctx.getString(R.string.owner_challenge_cooldown),
                                        Toast.LENGTH_LONG,
                                    ).show()
                                }
                            },
                            modifier = Modifier.alpha(if (challengeEnabled) 1f else 0.45f),
                        ) {
                            Icon(
                                Icons.Filled.Gavel,
                                contentDescription = stringResource(R.string.owner_challenge),
                                tint = if (challengeEnabled) MaterialTheme.colorScheme.onSurfaceVariant
                                else MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = 0.45f),
                            )
                        }
                    }
                    IconButton(onClick = { dialogs.confirmLeave = true }) {
                        Icon(
                            Icons.AutoMirrored.Filled.Logout,
                            contentDescription = stringResource(R.string.leave_group),
                            tint = MaterialTheme.colorScheme.error,
                        )
                    }
                }
            }
        }
    }

}

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
import androidx.compose.ui.graphics.ImageBitmap
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

internal class GroupDialogsState {
    var qrShown by mutableStateOf(false)
    var confirmLeave by mutableStateOf(false)
    var removeTarget by mutableStateOf<ByteArray?>(null)
    var showGraph by mutableStateOf(false)
    var editTarget by mutableStateOf<FfiMessage?>(null)
    var editText by mutableStateOf("")
    var actionTarget by mutableStateOf<FfiMessage?>(null)
    var reactionTarget by mutableStateOf<FfiMessage?>(null)
    var deleteTarget by mutableStateOf<FfiMessage?>(null)
    var showRename by mutableStateOf(false)
    var renameText by mutableStateOf("")
    var profileTarget by mutableStateOf<String?>(null)
    var reportTarget by mutableStateOf<ReportTarget?>(null)
    var showRequestInfo by mutableStateOf(false)
}

@Composable
internal fun rememberGroupDialogsState(): GroupDialogsState = remember { GroupDialogsState() }

internal class GroupDialogActions(
    val leaveGroup: () -> Unit,
    val removeMember: (ByteArray) -> Unit,
    val editMessage: (ByteArray, String) -> Unit,
    val deleteMessage: (ByteArray) -> Unit,
    val renameGroup: (String) -> Unit,
    val blockPeer: (ByteArray) -> Unit,
    val unblockPeer: (ByteArray) -> Unit,
    val reactionPicker: @Composable (FfiMessage, onDismiss: () -> Unit) -> Unit,
    val peerProfile: @Composable (
        hex: String,
        onClose: () -> Unit,
        onReport: (() -> Unit)?,
        blocked: Boolean,
        onToggleBlock: (() -> Unit)?,
    ) -> Unit,
    val reportDialog: @Composable (ReportTarget, onDismiss: () -> Unit) -> Unit,
    val graphDialog: @Composable (ByteArray, onDismiss: () -> Unit) -> Unit,
)

@Composable
internal fun GroupDialogs(
    chat: ChatUiState,
    state: GroupDialogsState,
    inviteQr: ImageBitmap?,
    actions: GroupDialogActions,
) {
    with(state) {
    if (showRequestInfo) {
        AlertDialog(
            onDismissRequest = { showRequestInfo = false },
            title = { Text(stringResource(R.string.owner_request)) },
            text = { Text(stringResource(R.string.owner_request_info)) },
            confirmButton = {
                TextButton(onClick = { showRequestInfo = false }) {
                    Text(stringResource(R.string.close))
                }
            },
        )
    }

    reactionTarget?.let { target ->
        actions.reactionPicker(target) { reactionTarget = null }
    }



    chat.currentGroup?.let { g -> if (showGraph) actions.graphDialog(g) { showGraph = false } }

    if (confirmLeave) {
        AlertDialog(
            onDismissRequest = { confirmLeave = false },
            title = { Text(stringResource(R.string.leave_group)) },
            text = { Text(stringResource(R.string.leave_group_confirm)) },
            confirmButton = {
                TextButton(onClick = { confirmLeave = false; actions.leaveGroup() }) {
                    Text(stringResource(R.string.leave), color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmLeave = false }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }

    if (qrShown) {
        AlertDialog(
            onDismissRequest = { qrShown = false },
            title = { Text(stringResource(R.string.invite_qr)) },
            text = {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    inviteQr?.let { bmp ->
                        Surface(color = Color.White, shape = RoundedCornerShape(12.dp)) {
                            Image(bmp, stringResource(R.string.invite_qr), modifier = Modifier.size(260.dp).padding(8.dp))
                        }
                    }
                    Spacer(Modifier.height(10.dp))
                    val kindLabel = if (chat.inviteMaxUses == 5) stringResource(R.string.invite_5)
                    else stringResource(R.string.invite_single)
                    Text(kindLabel, style = MaterialTheme.typography.bodyMedium)
                    chat.inviteAt?.let {
                        Text(
                            fmtTime(it),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { qrShown = false }) { Text(stringResource(R.string.close)) }
            },
        )
    }

    removeTarget?.let { pid ->
        AlertDialog(
            onDismissRequest = { removeTarget = null },
            title = { Text(stringResource(R.string.remove_member)) },
            text = { Text(stringResource(R.string.remove_member_confirm)) },
            confirmButton = {
                TextButton(onClick = { actions.removeMember(pid); removeTarget = null }) {
                    Text(stringResource(R.string.remove), color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { removeTarget = null }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
    editTarget?.let { em ->
        AlertDialog(
            onDismissRequest = { editTarget = null },
            title = { Text(stringResource(R.string.edit_message)) },
            text = {
                OutlinedTextField(
                    value = editText,
                    onValueChange = { editText = it },
                    singleLine = false,
                    modifier = Modifier.fillMaxWidth(),
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        actions.editMessage(em.id, editText.trim())
                        editTarget = null
                    },
                ) { Text(stringResource(R.string.save)) }
            },
            dismissButton = {
                TextButton(onClick = { editTarget = null }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
    actionTarget?.let { am ->
        Dialog(onDismissRequest = { actionTarget = null }) {
            Surface(
                shape = RoundedCornerShape(16.dp),
                tonalElevation = 8.dp,
                color = MaterialTheme.colorScheme.surface,
            ) {
                Column(Modifier.widthIn(min = 240.dp).padding(vertical = 8.dp)) {
                    TextButton(
                        onClick = {
                            reactionTarget = am
                            actionTarget = null
                        },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(
                            stringResource(R.string.reaction),
                            modifier = Modifier.weight(1f),
                            textAlign = TextAlign.Start,
                        )
                    }
                    if (am.author.toHex() == chat.myPeerId) {
                        TextButton(
                            onClick = {
                                actionTarget = null
                                editText = am.text
                                editTarget = am
                            },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(
                                stringResource(R.string.edit_message),
                                modifier = Modifier.weight(1f),
                                textAlign = TextAlign.Start,
                            )
                        }
                        TextButton(
                            onClick = {
                                actionTarget = null
                                deleteTarget = am
                            },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(
                                stringResource(R.string.delete_message),
                                modifier = Modifier.weight(1f),
                                textAlign = TextAlign.Start,
                                color = MaterialTheme.colorScheme.error,
                            )
                        }
                    } else {
                        TextButton(
                            onClick = {
                                actionTarget = null
                                reportTarget = ReportTarget(
                                    am.author,
                                    chat.currentGroup ?: ByteArray(0),
                                    am.id,
                                )
                            },
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(
                                stringResource(R.string.report),
                                modifier = Modifier.weight(1f),
                                textAlign = TextAlign.Start,
                                color = MaterialTheme.colorScheme.error,
                            )
                        }
                    }
                }
            }
        }
    }
    deleteTarget?.let { dm ->
        AlertDialog(
            onDismissRequest = { deleteTarget = null },
            title = { Text(stringResource(R.string.delete_message)) },
            text = { Text(stringResource(R.string.delete_message_confirm)) },
            confirmButton = {
                TextButton(onClick = { actions.deleteMessage(dm.id); deleteTarget = null }) {
                    Text(stringResource(R.string.remove), color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { deleteTarget = null }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
    profileTarget?.let { hex ->
        actions.peerProfile(
            hex,
            { profileTarget = null },
            if (hex != chat.myPeerId) {
                {
                    val pid = hexToBytes(hex)
                    if (pid != null) {
                        profileTarget = null
                        reportTarget = ReportTarget(pid, chat.currentGroup ?: ByteArray(0), ByteArray(0))
                    }
                }
            } else null,
            chat.blocked.contains(hex),
            if (hex != chat.myPeerId) {
                {
                    val pid = hexToBytes(hex)
                    if (pid != null) {
                        if (chat.blocked.contains(hex)) actions.unblockPeer(pid) else actions.blockPeer(pid)
                        profileTarget = null
                    }
                }
            } else null,
        )
    }
    reportTarget?.let { t -> actions.reportDialog(t) { reportTarget = null } }

    if (showRename) {
        AlertDialog(
            onDismissRequest = { showRename = false },
            title = { Text(stringResource(R.string.rename_group)) },
            text = {
                OutlinedTextField(
                    value = renameText,
                    onValueChange = { renameText = it },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            },
            confirmButton = {
                TextButton(onClick = { actions.renameGroup(renameText); showRename = false }) {
                    Text(stringResource(R.string.save))
                }
            },
            dismissButton = {
                TextButton(onClick = { showRename = false }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
    }
}

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

// --- Список групп --------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun GroupsScreen(vm: ChatViewModel, onSettings: () -> Unit) {
    val state by vm.uiState.collectAsState()
    var showAdd by remember { mutableStateOf(false) }

    Box(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (state.groupInfos.isEmpty()) {
                Text(
                    stringResource(R.string.groups_empty),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 24.dp),
                )
            } else {
                state.groupInfos.forEach { gi ->
                    val key = gi.id.toHex()
                    Card(modifier = Modifier.fillMaxWidth().clickable { vm.selectGroup(gi.id) }) {
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Avatar(state.groupAvatars[key], key, 64.dp)
                            Spacer(Modifier.width(12.dp))
                            Column {
                                Text(gi.name, style = MaterialTheme.typography.bodyLarge)
                                Text(
                                    pluralStringResource(R.plurals.members_count, state.memberCounts[key] ?: 1, state.memberCounts[key] ?: 1),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            Spacer(Modifier.weight(1f))
                            val u = state.unread[key] ?: 0uL
                            if (u > 0uL) {
                                Badge(containerColor = MaterialTheme.colorScheme.error) {
                                    Text(u.toString())
                                }
                            }
                        }
                    }
                }
            }
            Spacer(Modifier.size(72.dp)) // место под кнопки
        }

        Row(
            modifier = Modifier.align(Alignment.BottomStart).padding(16.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            BadgedBox(
                badge = {
                    if (vm.update != null) {
                        Badge(containerColor = Color.Red)
                    }
                },
            ) {
                SmallFloatingActionButton(
                    onClick = onSettings,
                    containerColor = MaterialTheme.colorScheme.surfaceVariant,
                ) {
                    Icon(Icons.Filled.Settings, contentDescription = stringResource(R.string.settings))
                }
            }
            FloatingActionButton(onClick = { showAdd = true }) {
                Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.add_group))
            }
        }
    }

    if (showAdd) AddGroupDialog(vm, onClose = { showAdd = false })
}

@Composable
internal fun AddGroupDialog(vm: ChatViewModel, onClose: () -> Unit) {
    var name by remember { mutableStateOf("") }
    var inviteIn by remember { mutableStateOf("") }

    AlertDialog(
        onDismissRequest = onClose,
        title = { Text(stringResource(R.string.new_group)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text(stringResource(R.string.group_name_hint)) },
                    singleLine = true,
                )
                Button(
                    onClick = { vm.createGroup(name); onClose() },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(stringResource(R.string.create)) }
                OutlinedTextField(
                    value = inviteIn,
                    onValueChange = { inviteIn = it },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text(stringResource(R.string.or_paste_invite)) },
                    maxLines = 3,
                )
                OutlinedButton(
                    onClick = { vm.joinGroup(inviteIn.trim()); onClose() },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text(stringResource(R.string.join_by_invite)) }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onClose) { Text(stringResource(R.string.close)) } },
    )
}

package com.p2pchat.app

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.core.content.ContextCompat
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.text.ClickableText
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.GifBox
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.flow.collect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import com.p2pchat.app.gif.GifAttachmentView
import com.p2pchat.app.gif.GifPickerDialog
import uniffi.p2pchat_ffi.FfiMessage

class MainActivity : ComponentActivity() {
    private val vm: ChatViewModel by viewModels()

    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(Lang.wrap(newBase))
    }

    private val notifPerm =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    Root(vm)
                }
            }
        }
        handleIntent(intent)
        maybeRequestNotifications()
    }

    override fun onStart() {
        super.onStart()
        // Вернулись в приложение — догоняем пропущенное (вариант B).
        vm.onForeground()
    }

    override fun onStop() {
        super.onStop()
        vm.setVisible(false)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleIntent(intent)
    }

    private fun handleIntent(intent: Intent?) {
        intent?.getStringExtra(EXTRA_SHARED_TEXT)?.let(vm::onSharedText)
        intent?.getStringExtra(EXTRA_GROUP)?.let { vm.selectGroup(it.toByteArray()) }
    }

    private fun maybeRequestNotifications() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            notifPerm.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    companion object {
        /** Ссылка/текст, которым поделились с приложением (ACTION_SEND). */
        const val EXTRA_SHARED_TEXT = "extra_shared_text"

        /** Группа, которую открыть (из уведомления). */
        const val EXTRA_GROUP = "extra_group"
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Root(vm: ChatViewModel) {
    var settings by remember { mutableStateOf(false) }
    val group = vm.currentGroup

    val title = when {
        settings -> stringResource(R.string.settings)
        group != null -> String(group)
        else -> "p2pchat"
    }
    val onBack: (() -> Unit)? = when {
        settings -> { { settings = false } }
        group != null -> { { vm.backToList() } }
        else -> null
    }

    Scaffold(topBar = {
        TopAppBar(
            title = { Text(title) },
            navigationIcon = {
                if (onBack != null) {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
                    }
                }
            },
        )
    }) { pad ->
        Box(modifier = Modifier.fillMaxSize().padding(pad)) {
            when {
                settings -> SettingsScreen(vm)
                group != null -> GroupScreen(vm)
                else -> GroupsScreen(vm, onSettings = { settings = true })
            }
        }
    }
}

// --- Список групп --------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun GroupsScreen(vm: ChatViewModel, onSettings: () -> Unit) {
    var showAdd by remember { mutableStateOf(false) }

    Box(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            if (vm.groups.isEmpty()) {
                Text(
                    stringResource(R.string.groups_empty),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 24.dp),
                )
            } else {
                vm.groups.forEach { name ->
                    Card(modifier = Modifier.fillMaxWidth().clickable { vm.selectGroup(name.toByteArray()) }) {
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(16.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Column {
                                Text(name, style = MaterialTheme.typography.bodyLarge)
                                Text(
                                    stringResource(R.string.members_count, vm.memberCounts[name] ?: 1),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                            Spacer(Modifier.weight(1f))
                            val u = vm.unread[name] ?: 0uL
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
private fun AddGroupDialog(vm: ChatViewModel, onClose: () -> Unit) {
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

// --- Настройки / профиль -------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsScreen(vm: ChatViewModel) {
    val ctx = LocalContext.current
    var showUpdate by remember { mutableStateOf(false) }
    val update = vm.update
    var nick by remember(vm.myNickname) { mutableStateOf(vm.myNickname ?: "") }
    var editingNick by remember(vm.myNickname) { mutableStateOf(false) }
    val noneNick = stringResource(R.string.nickname_none)
    var showLang by remember { mutableStateOf(false) }
    var explainKind by remember { mutableStateOf<String?>(null) }

    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Версия и наличие обновления. Тап — проверить и открыть, что нового.
        Card {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable {
                        val known = vm.update
                        if (known != null) {
                            showUpdate = true
                        } else {
                            vm.checkNow { has ->
                                if (has) showUpdate = true
                                else Toast.makeText(ctx, ctx.getString(R.string.up_to_date), Toast.LENGTH_SHORT).show()
                            }
                        }
                    }
                    .padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(stringResource(R.string.version), style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.weight(1f))
                Text(
                    "v" + vm.version,
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                if (update != null) {
                    Spacer(Modifier.width(8.dp))
                    Text(
                        "→ " + update.version,
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.primary,
                    )
                    Spacer(Modifier.width(8.dp))
                    Badge(containerColor = Color.Red)
                }
            }
        }

        Card {
            Column(modifier = Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(stringResource(R.string.nickname), style = MaterialTheme.typography.titleSmall)
                    Spacer(Modifier.weight(1f))
                    if (editingNick) {
                        TextButton(onClick = { vm.setNickname(nick); editingNick = false }) { Text(stringResource(R.string.save)) }
                    } else {
                        TextButton(onClick = { nick = vm.myNickname ?: ""; editingNick = true }) { Text(stringResource(R.string.change)) }
                    }
                }
                if (editingNick) {
                    OutlinedTextField(
                        value = nick,
                        onValueChange = { nick = it },
                        modifier = Modifier.fillMaxWidth(),
                        placeholder = { Text(stringResource(R.string.nickname_hint)) },
                        singleLine = true,
                    )
                } else {
                    Text(
                        nick.ifBlank { noneNick },
                        style = MaterialTheme.typography.bodyLarge,
                    )
                }
            }
        }

        Card {
            Row(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.keep_alive_title), style = MaterialTheme.typography.titleSmall)
                    Text(
                        stringResource(R.string.keep_alive_desc),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Spacer(Modifier.width(8.dp))
                Switch(
                    checked = vm.backgroundEnabled,
                    onCheckedChange = { vm.setBackground(it) },
                )
            }
        }

        Card {
            Row(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.debug_title), style = MaterialTheme.typography.titleSmall)
                    Text(
                        stringResource(R.string.debug_desc),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Spacer(Modifier.width(8.dp))
                Switch(
                    checked = vm.debugLogsEnabled,
                    onCheckedChange = { vm.setDebugLogs(it) },
                )
            }
        }

        Card {
            Row(
                modifier = Modifier.fillMaxWidth().clickable { showLang = true }.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(stringResource(R.string.language), style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.weight(1f))
                val cur = Lang.LANGUAGES.firstOrNull { it.first == Lang.get(ctx) } ?: Lang.LANGUAGES.first()
                Text("${cur.second}  ${cur.third}", style = MaterialTheme.typography.bodyLarge)
            }
        }

        Card {
            Column(modifier = Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(stringResource(R.string.connection), style = MaterialTheme.typography.titleSmall)
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        stringResource(R.string.connection_type),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Spacer(Modifier.weight(1f))
                    ConnTypeChip("STUN", active = vm.directCount > 0, color = Color(0xFF2E9E6B)) {
                        explainKind = if (explainKind == "stun") null else "stun"
                    }
                    Spacer(Modifier.width(10.dp))
                    ConnTypeChip("TURN", active = vm.turnCount > 0, color = Color(0xFFE08A1E)) {
                        explainKind = if (explainKind == "turn") null else "turn"
                    }
                }
                Text(
                    "STUN ${vm.directCount} · TURN ${vm.turnCount}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                explainKind?.let { ConnExplanation(it) }
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        stringResource(R.string.connection_last_signaling),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Spacer(Modifier.weight(1f))
                    Text(fmtTime(vm.lastSignalingTs), style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }

    if (showLang) {
        AlertDialog(
            onDismissRequest = { showLang = false },
            title = { Text(stringResource(R.string.language)) },
            text = {
                Column(Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
                    val cur = Lang.get(ctx)
                    Lang.LANGUAGES.forEach { (code, flag, name) ->
                        Row(
                            modifier = Modifier.fillMaxWidth()
                                .clickable {
                                    Lang.set(ctx, code)
                                    showLang = false
                                    (ctx as? Activity)?.recreate()
                                }
                                .padding(vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                "$flag  $name",
                                style = MaterialTheme.typography.bodyLarge,
                                color = if (code == cur) MaterialTheme.colorScheme.primary
                                else MaterialTheme.colorScheme.onSurface,
                                modifier = Modifier.weight(1f),
                            )
                            if (code == cur) Icon(Icons.Filled.Check, contentDescription = null)
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { showLang = false }) { Text(stringResource(R.string.close)) }
            },
        )
    }

    if (showUpdate && update != null) {
        UpdateDialog(update, onDismiss = { showUpdate = false })
    }
}

/** Кликабельная метка типа соединения (STUN/TURN). */
@Composable
private fun ConnTypeChip(label: String, active: Boolean, color: Color, onClick: () -> Unit) {
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
private fun ConnExplanation(kind: String) {
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

/** Время последней активности сигналинга в виде «HH:mm:ss» (или «—»). */
private fun fmtTime(ts: Long): String {
    if (ts <= 0L) return "—"
    return java.text.SimpleDateFormat("HH:mm:ss", java.util.Locale.getDefault())
        .format(java.util.Date(ts))
}

@Composable
private fun UpdateDialog(info: AppUpdate, onDismiss: () -> Unit) {
    val ctx = LocalContext.current
    var expanded by remember { mutableStateOf(false) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.update_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.update_text, info.version))
                if (info.changelog.isNotEmpty()) {
                    Row(
                        modifier = Modifier.fillMaxWidth().clickable { expanded = !expanded },
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(stringResource(R.string.whats_new), style = MaterialTheme.typography.titleSmall)
                        Spacer(Modifier.weight(1f))
                        Icon(
                            if (expanded) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown,
                            contentDescription = stringResource(R.string.expand),
                        )
                    }
                    if (expanded) {
                        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            info.changelog.forEach { group ->
                                Text(
                                    group.title,
                                    style = MaterialTheme.typography.titleSmall,
                                    color = MaterialTheme.colorScheme.primary,
                                )
                                group.items.forEach { item ->
                                    Text(
                                        "• $item",
                                        style = MaterialTheme.typography.bodySmall,
                                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                runCatching {
                    ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(info.downloadUrl)))
                }
                onDismiss()
            }) { Text(stringResource(R.string.download)) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.later)) }
        },
    )
}

// --- Экран группы --------------------------------------------------------

@Composable
private fun GroupScreen(vm: ChatViewModel) {
    var input by remember { mutableStateOf("") }
    var inviteExpanded by remember { mutableStateOf(false) }
    var confirmLeave by remember { mutableStateOf(false) }
    var showGifPicker by remember { mutableStateOf(false) }
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    val invite = vm.invite
    val inviteQr = remember(invite) { invite?.let { generateQrBitmap(it, 640) } }

    // Черновик из «Поделиться»: один раз подставляем в поле ввода.
    val shared = vm.sharedDraft
    LaunchedEffect(shared) {
        if (shared != null) {
            input = shared
            vm.consumeSharedDraft()
        }
    }

    // Список сообщений: reverseLayout=true — «дно» это самые свежие сообщения.
    val listState = rememberLazyListState()
    val groupKey = vm.currentGroup?.toHex()
    LaunchedEffect(groupKey) { if (vm.messages.isNotEmpty()) listState.scrollToItem(0) }

    // Новое сообщение/гифка: если это наше или мы у «дна» — прокручиваем вниз.
    val newestId = vm.messages.lastOrNull()?.id?.toHex()
    LaunchedEffect(newestId) {
        val newest = vm.messages.lastOrNull() ?: return@LaunchedEffect
        val mine = newest.author.toHex() == vm.peerId
        val nearBottom = listState.firstVisibleItemIndex <= 1
        if (mine || nearBottom) listState.animateScrollToItem(0)
    }

    // Авто-подгрузка старых при прокрутке вверх (дошли до начала окна).
    LaunchedEffect(listState) {
        snapshotFlow {
            val info = listState.layoutInfo
            val last = info.visibleItemsInfo.lastOrNull()?.index ?: -1
            info.totalItemsCount > 0 && last >= info.totalItemsCount - 3
        }.collect { near -> if (near && vm.hasMore) vm.loadOlder() }
    }

    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Свёрнутый по умолчанию блок приглашения
        Card {
            Row(
                modifier = Modifier.fillMaxWidth().clickable { inviteExpanded = !inviteExpanded }.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(stringResource(R.string.invite_to_group), style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.weight(1f))
                Text(stringResource(R.string.members_count, vm.members.size), style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
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
                    if (inviteQr != null) {
                        Surface(color = Color.White, shape = RoundedCornerShape(12.dp)) {
                            Image(inviteQr, stringResource(R.string.invite_qr), modifier = Modifier.size(240.dp).padding(10.dp))
                        }
                    }
                    if (invite != null) {
                        OutlinedButton(onClick = {
                            clipboard.setText(AnnotatedString(invite))
                            Toast.makeText(ctx, ctx.getString(R.string.invite_copied), Toast.LENGTH_SHORT).show()
                        }) { Text(stringResource(R.string.copy_invite)) }
                    }
                    Text(
                        stringResource(R.string.status_connected, vm.status, vm.connected),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(stringResource(R.string.participants), style = MaterialTheme.typography.titleSmall)
                        vm.members.forEach { m ->
                            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                                val hex = m.peerId.toHex()
                                val label = (vm.nicknames[hex]?.takeIf { it.isNotBlank() }
                                    ?: hex.take(10)) + (if (hex == vm.peerId) stringResource(R.string.you) else "")
                                Text(label, style = MaterialTheme.typography.bodySmall)
                                Spacer(Modifier.weight(1f))
                                val conn = vm.peerKinds[m.publicKey.toHex()]
                                if (conn == "turn" || conn == "direct") {
                                    Text(
                                        if (conn == "turn") "TURN" else "STUN",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = if (conn == "turn") Color(0xFFE08A1E) else Color(0xFF2E9E6B),
                                    )
                                    Spacer(Modifier.width(8.dp))
                                }
                                if (vm.isOwner && hex != vm.peerId) {
                                    IconButton(onClick = { vm.removeMember(m.peerId) }) {
                                        Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.remove))
                                    }
                                }
                            }
                        }
                    }
                    OutlinedButton(
                        onClick = { confirmLeave = true },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(stringResource(R.string.leave_group), color = MaterialTheme.colorScheme.error)
                    }
                }
            }
        }

        Box(
            modifier = Modifier.fillMaxWidth().weight(1f)
                .background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(12.dp)),
        ) {
            if (vm.messages.isEmpty()) {
                Text(stringResource(R.string.no_messages), style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(12.dp))
            } else {
                LazyColumn(
                    state = listState,
                    reverseLayout = true,
                    modifier = Modifier.fillMaxSize().padding(horizontal = 12.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    items(vm.messages.asReversed(), key = { it.id.toHex() }) { m ->
                        val mine = m.author.toHex() == vm.peerId
                        val name = vm.nicknames[m.author.toHex()]?.takeIf { it.isNotBlank() }
                            ?: m.author.toHexShort()
                        MessageRow(mine = mine, name = name, message = m, vm = vm)
                    }
                    if (vm.hasMore) {
                        item {
                            TextButton(onClick = { vm.loadOlder() }, modifier = Modifier.fillMaxWidth()) {
                                Text(stringResource(R.string.show_earlier))
                            }
                        }
                    }
                }
            }
        }

        Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            if (vm.gifEnabled) {
                IconButton(onClick = { showGifPicker = true }) {
                    Icon(Icons.Filled.GifBox, contentDescription = stringResource(R.string.gif))
                }
            }
            OutlinedTextField(
                value = input,
                onValueChange = { input = it },
                modifier = Modifier.weight(1f),
                placeholder = { Text(stringResource(R.string.message_hint)) },
                singleLine = true,
            )
            Spacer(Modifier.width(8.dp))
            Button(onClick = { vm.send(input); input = "" }) { Text(stringResource(R.string.send)) }
        }
    }

    if (showGifPicker && vm.gifEnabled) {
        GifPickerDialog(vm = vm, onDismiss = { showGifPicker = false })
    }

    if (confirmLeave) {
        AlertDialog(
            onDismissRequest = { confirmLeave = false },
            title = { Text(stringResource(R.string.leave_group)) },
            text = { Text(stringResource(R.string.leave_group_confirm)) },
            confirmButton = {
                TextButton(onClick = { confirmLeave = false; vm.leaveGroup() }) {
                    Text(stringResource(R.string.leave), color = MaterialTheme.colorScheme.error)
                }
            },
            dismissButton = {
                TextButton(onClick = { confirmLeave = false }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
}

private fun ByteArray.toHexShort(): String =
    joinToString("") { "%02x".format(it) }.take(6)

private const val URL_TAG = "url"

/** Пузырь сообщения: владелец — справа, собеседник — слева. */
@Composable
private fun MessageRow(mine: Boolean, name: String, message: FfiMessage, vm: ChatViewModel) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
        horizontalArrangement = if (mine) Arrangement.End else Arrangement.Start,
    ) {
        Surface(
            color = if (mine) MaterialTheme.colorScheme.primaryContainer
            else MaterialTheme.colorScheme.surface,
            shape = RoundedCornerShape(12.dp),
            tonalElevation = 1.dp,
            modifier = Modifier.widthIn(max = 260.dp),
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
                if (att != null) {
                    GifAttachmentView(
                        providerId = att.id,
                        width = att.width.toInt(),
                        height = att.height.toInt(),
                        vm = vm,
                    )
                } else {
                    val url = singleUrl(message.text)
                    if (url != null) {
                        LinkPreviewCard(url) { MessageText(message.text) }
                    } else {
                        MessageText(message.text)
                    }
                }
            }
        }
    }
}

/** Текст сообщения с кликабельными ссылками http(s)://… */
@Composable
private fun MessageText(text: String) {
    val ctx = LocalContext.current
    val annotated = remember(text) { linksAnnotated(text) }
    ClickableText(
        text = annotated,
        style = MaterialTheme.typography.bodyMedium.copy(color = MaterialTheme.colorScheme.onSurface),
    ) { offset ->
        annotated.getStringAnnotations(URL_TAG, offset, offset).firstOrNull()?.let { ann ->
            runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(ann.item))) }
        }
    }
}

/** Если всё сообщение — одна ссылка (без текста), вернуть её; иначе null. */
private fun singleUrl(text: String): String? {
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
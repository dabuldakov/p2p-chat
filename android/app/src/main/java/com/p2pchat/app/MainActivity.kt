package com.p2pchat.app

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.AlertDialog
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
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    Root(viewModel())
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun Root(vm: ChatViewModel = viewModel()) {
    var settings by remember { mutableStateOf(false) }
    val group = vm.currentGroup

    val title = when {
        settings -> "Настройки"
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
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Назад")
                    }
                }
            },
            actions = {
                Text(
                    "v" + BuildConfig.VERSION_NAME,
                    modifier = Modifier.padding(end = 16.dp),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
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
                    "Пока нет групп. Нажмите «+», чтобы создать или войти по инвайту.",
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
                            Text(name, style = MaterialTheme.typography.bodyLarge)
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
            SmallFloatingActionButton(
                onClick = onSettings,
                containerColor = MaterialTheme.colorScheme.surfaceVariant,
            ) {
                Icon(Icons.Filled.Settings, contentDescription = "Настройки")
            }
            FloatingActionButton(onClick = { showAdd = true }) {
                Icon(Icons.Filled.Add, contentDescription = "Добавить группу")
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
        title = { Text("Новая группа") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("название группы") },
                    singleLine = true,
                )
                Button(
                    onClick = { vm.createGroup(name); onClose() },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Создать") }
                OutlinedTextField(
                    value = inviteIn,
                    onValueChange = { inviteIn = it },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("или вставьте инвайт") },
                    maxLines = 3,
                )
                OutlinedButton(
                    onClick = { vm.joinGroup(inviteIn.trim()); onClose() },
                    modifier = Modifier.fillMaxWidth(),
                ) { Text("Войти по инвайту") }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onClose) { Text("Закрыть") } },
    )
}

// --- Настройки / профиль -------------------------------------------------

@Composable
private fun SettingsScreen(vm: ChatViewModel) {
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    val qr = remember { generateQrBitmap(vm.publicKeyHex, 480) }

    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("Профиль", style = MaterialTheme.typography.titleMedium)
        Surface(color = Color.White, shape = RoundedCornerShape(12.dp)) {
            Image(qr, "QR идентификатора", modifier = Modifier.size(240.dp).padding(12.dp))
        }
        Text(
            "id: ${vm.peerId.take(24)}…",
            style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        OutlinedButton(onClick = {
            clipboard.setText(AnnotatedString(vm.publicKeyHex))
            Toast.makeText(ctx, "Ключ скопирован", Toast.LENGTH_SHORT).show()
        }) { Text("Скопировать ключ") }

        Text(
            vm.publicKeyHex,
            style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(horizontal = 8.dp),
        )
    }
}

// --- Экран группы --------------------------------------------------------

@Composable
private fun GroupScreen(vm: ChatViewModel) {
    var input by remember { mutableStateOf("") }
    var inviteExpanded by remember { mutableStateOf(false) }
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    val invite = vm.invite
    val inviteQr = remember(invite) { invite?.let { generateQrBitmap(it, 640) } }

    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Свёрнутый по умолчанию блок приглашения
        Card {
            Row(
                modifier = Modifier.fillMaxWidth().clickable { inviteExpanded = !inviteExpanded }.padding(16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text("Пригласить в группу", style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.weight(1f))
                Text("${vm.members.size} участн.", style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
                Spacer(Modifier.width(8.dp))
                Icon(
                    if (inviteExpanded) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown,
                    contentDescription = "Развернуть",
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
                            Image(inviteQr, "QR инвайта", modifier = Modifier.size(240.dp).padding(10.dp))
                        }
                    }
                    if (invite != null) {
                        OutlinedButton(onClick = {
                            clipboard.setText(AnnotatedString(invite))
                            Toast.makeText(ctx, "Инвайт скопирован", Toast.LENGTH_SHORT).show()
                        }) { Text("Скопировать инвайт") }
                    }
                    Text(
                        "${vm.status} · соединено: ${vm.connected}",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }

        Box(
            modifier = Modifier.fillMaxWidth()
                .background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(12.dp))
                .padding(12.dp),
        ) {
            if (vm.messages.isEmpty()) {
                Text("Пока нет сообщений", style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    vm.messages.forEach { m ->
                        Text("${m.author.toHexShort()}: ${m.text}")
                    }
                }
            }
        }

        Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = input,
                onValueChange = { input = it },
                modifier = Modifier.weight(1f),
                placeholder = { Text("сообщение…") },
                singleLine = true,
            )
            Spacer(Modifier.width(8.dp))
            Button(onClick = { vm.send(input); input = "" }) { Text("Отправить") }
        }
    }
}

private fun ByteArray.toHexShort(): String =
    joinToString("") { "%02x".format(it) }.take(6)
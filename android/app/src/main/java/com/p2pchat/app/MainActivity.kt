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
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
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
    val g = vm.currentGroup
    Scaffold(topBar = {
        TopAppBar(
            title = { Text(if (g == null) "p2pchat" else String(g)) },
            navigationIcon = {
                if (g != null) {
                    TextButton(onClick = { vm.backToList() }) { Text("←") }
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
            if (g == null) GroupsScreen(vm) else GroupScreen(vm)
        }
    }
}

// --- Список групп --------------------------------------------------------

@Composable
private fun GroupsScreen(vm: ChatViewModel) {
    var newName by remember { mutableStateOf("") }
    var inviteIn by remember { mutableStateOf("") }
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    val myQr = remember { generateQrBitmap(vm.publicKeyHex, 360) }

    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant)) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text("Мой ключ", style = MaterialTheme.typography.titleMedium)
                Surface(color = Color.White, shape = RoundedCornerShape(12.dp)) {
                    Image(myQr, "QR ключа", modifier = Modifier.size(160.dp).padding(8.dp))
                }
                OutlinedButton(onClick = {
                    clipboard.setText(AnnotatedString(vm.publicKeyHex))
                    Toast.makeText(ctx, "Ключ скопирован", Toast.LENGTH_SHORT).show()
                }) { Text("Скопировать ключ") }
            }
        }

        Card {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Создать группу", style = MaterialTheme.typography.titleSmall)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = newName,
                        onValueChange = { newName = it },
                        modifier = Modifier.weight(1f),
                        placeholder = { Text("название") },
                        singleLine = true,
                    )
                    Spacer(Modifier.width(8.dp))
                    Button(onClick = { vm.createGroup(newName); newName = "" }) { Text("Создать") }
                }
            }
        }

        Card {
            Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Войти по инвайту", style = MaterialTheme.typography.titleSmall)
                OutlinedTextField(
                    value = inviteIn,
                    onValueChange = { inviteIn = it },
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text("p2pchat://join?g=…&k=…") },
                    maxLines = 3,
                )
                Button(onClick = { vm.joinGroup(inviteIn.trim()); inviteIn = "" }) { Text("Войти") }
            }
        }

        Text("Мои группы", style = MaterialTheme.typography.titleSmall)
        if (vm.groups.isEmpty()) {
            Text("Пока нет групп", style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            vm.groups.forEach { name ->
                Card(modifier = Modifier.fillMaxWidth().clickable { vm.selectGroup(name.toByteArray()) }) {
                    Text(name, modifier = Modifier.padding(16.dp), style = MaterialTheme.typography.bodyLarge)
                }
            }
        }
    }
}

// --- Экран группы --------------------------------------------------------

@Composable
private fun GroupScreen(vm: ChatViewModel) {
    var input by remember { mutableStateOf("") }
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current
    val invite = vm.invite
    val inviteQr = remember(invite) { invite?.let { generateQrBitmap(it, 480) } }

    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp).verticalScroll(rememberScrollState()),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Card(colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant)) {
            Column(
                modifier = Modifier.fillMaxWidth().padding(16.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text("Пригласить в группу", style = MaterialTheme.typography.titleSmall)
                if (inviteQr != null) {
                    Surface(color = Color.White, shape = RoundedCornerShape(12.dp)) {
                        Image(inviteQr, "QR инвайта", modifier = Modifier.size(200.dp).padding(8.dp))
                    }
                }
                if (invite != null) {
                    OutlinedButton(onClick = {
                        clipboard.setText(AnnotatedString(invite))
                        Toast.makeText(ctx, "Инвайт скопирован", Toast.LENGTH_SHORT).show()
                    }) { Text("Скопировать инвайт") }
                }
                Text("участников: ${vm.members.size}", style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("${vm.status} · соединено: ${vm.connected}", style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant)
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
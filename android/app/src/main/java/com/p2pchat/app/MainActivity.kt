package com.p2pchat.app

import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
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
                    ChatScreen(viewModel())
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(vm: ChatViewModel = viewModel()) {
    var input by remember { mutableStateOf("") }
    var peerKey by remember { mutableStateOf("") }
    val clipboard = LocalClipboardManager.current
    val ctx = LocalContext.current

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("p2pchat") },
                actions = {
                    Text(
                        "v" + BuildConfig.VERSION_NAME,
                        modifier = Modifier.padding(end = 16.dp),
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                },
            )
        }
    ) { pad ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(pad)
                .padding(horizontal = 16.dp)
                .verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            KeyCard(
                peerId = vm.peerId,
                keyHex = vm.publicKeyHex,
                onCopy = {
                    clipboard.setText(AnnotatedString(vm.publicKeyHex))
                    Toast.makeText(ctx, "Ключ скопирован", Toast.LENGTH_SHORT).show()
                },
            )

            ConnectRow(
                value = peerKey,
                onValueChange = { peerKey = it },
                status = vm.status,
                onConnect = { vm.connectTo(peerKey) },
            )

            Text("Сообщения", style = MaterialTheme.typography.titleSmall)
            Messages(list = vm.messages)

            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
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
            Spacer(Modifier.size(8.dp))
        }
    }
}

@Composable
private fun KeyCard(peerId: String, keyHex: String, onCopy: () -> Unit) {
    val qr = remember(keyHex) { generateQrBitmap(keyHex) }

    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceVariant),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text("Ваш ключ", style = MaterialTheme.typography.titleMedium)
            Text(
                "Покажите QR или передайте ключ собеседнику",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )

            Surface(color = Color.White, shape = RoundedCornerShape(12.dp)) {
                Image(
                    bitmap = qr,
                    contentDescription = "QR-код вашего ключа",
                    modifier = Modifier.size(220.dp).padding(12.dp),
                )
            }

            Text(
                keyHex,
                style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                textAlign = TextAlign.Center,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )

            OutlinedButton(onClick = onCopy, modifier = Modifier.fillMaxWidth()) {
                Text("Скопировать ключ")
            }

            Text(
                "id: ${peerId.take(16)}…",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun ConnectRow(value: String, onValueChange: (String) -> Unit, status: String, onConnect: () -> Unit) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(modifier = Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Подключиться к собеседнику", style = MaterialTheme.typography.titleSmall)
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = value,
                    onValueChange = onValueChange,
                    modifier = Modifier.weight(1f),
                    placeholder = { Text("ключ собеседника (hex)") },
                    singleLine = true,
                )
                Spacer(Modifier.width(8.dp))
                Button(onClick = onConnect) { Text("Connect") }
            }
            Text(status, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
private fun Messages(list: List<uniffi.p2pchat_ffi.FfiMessage>) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(12.dp))
            .padding(12.dp),
    ) {
        if (list.isEmpty()) {
            Text(
                "Пока нет сообщений",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                list.forEach { m ->
                    Text("${m.authorShort()}: ${m.text}")
                }
            }
        }
    }
}

private fun uniffi.p2pchat_ffi.FfiMessage.authorShort(): String =
    author.joinToString("") { "%02x".format(it) }.take(6)

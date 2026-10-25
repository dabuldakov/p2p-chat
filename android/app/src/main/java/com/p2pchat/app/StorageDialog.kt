package com.p2pchat.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import android.widget.Toast

internal fun formatBytes(bytes: Long): String = when {
    bytes >= 1024L * 1024 * 1024 -> "%.1f GB".format(bytes / (1024.0 * 1024 * 1024))
    bytes >= 1024L * 1024 -> "%.1f MB".format(bytes / (1024.0 * 1024))
    bytes >= 1024L -> "%.0f KB".format(bytes / 1024.0)
    else -> "$bytes B"
}

@Composable
fun StorageDialog(vm: ChatViewModel, onDismiss: () -> Unit) {
    val ctx = LocalContext.current
    var stats by remember { mutableStateOf(vm.storageStats()) }
    var confirmAll by remember { mutableStateOf(false) }
    val total = stats.sumOf { it.size }

    fun refreshStats() {
        stats = vm.storageStats()
        Toast.makeText(ctx, ctx.getString(R.string.storage_cleared), Toast.LENGTH_SHORT).show()
    }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(18.dp),
            color = MaterialTheme.colorScheme.surface,
            tonalElevation = 8.dp,
            modifier = Modifier.fillMaxWidth(0.94f),
        ) {
            Column(Modifier.padding(14.dp)) {
                Text(stringResource(R.string.storage_title), style = MaterialTheme.typography.titleMedium)
                Row(Modifier.fillMaxWidth().padding(vertical = 8.dp)) {
                    Text(stringResource(R.string.storage_total), style = MaterialTheme.typography.bodySmall)
                    Spacer(Modifier.weight(1f))
                    Text(formatBytes(total), style = MaterialTheme.typography.bodySmall)
                }
                HorizontalDivider()
                LazyColumn(
                    Modifier.fillMaxWidth().heightIn(max = 420.dp).padding(vertical = 4.dp),
                ) {
                    items(stats.filter { it.size > 0 }) { row ->
                        Row(
                            Modifier.fillMaxWidth().clickable {
                                vm.clearStorage(row.id)
                                refreshStats()
                            }.padding(vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                stringResource(labelFor(row.id)),
                                style = MaterialTheme.typography.bodyMedium,
                                modifier = Modifier.weight(1f),
                            )
                            Text(
                                formatBytes(row.size),
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            Spacer(Modifier.width(6.dp))
                            IconButton(onClick = {
                                vm.clearStorage(row.id)
                                refreshStats()
                            }) {
                                Icon(Icons.Filled.Delete, contentDescription = stringResource(R.string.storage_clear))
                            }
                        }
                    }
                    if (stats.all { it.size == 0L }) {
                        item {
                            Text(
                                stringResource(R.string.storage_cleared),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                modifier = Modifier.padding(12.dp),
                            )
                        }
                    }
                }
                Text(
                    stringResource(R.string.storage_media_note),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(vertical = 4.dp),
                )
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                ) {
                    TextButton(onClick = { confirmAll = true }) {
                        Text(stringResource(R.string.storage_clear_all), color = MaterialTheme.colorScheme.error)
                    }
                    TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
                }
            }
        }
    }

    if (confirmAll) {
        AlertDialog(
            onDismissRequest = { confirmAll = false },
            title = { Text(stringResource(R.string.storage_clear_all)) },
            text = { Text(stringResource(R.string.storage_clear_confirm)) },
            confirmButton = {
                TextButton(onClick = {
                    confirmAll = false
                    vm.clearStorage("all")
                    refreshStats()
                }) { Text(stringResource(R.string.storage_clear), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = {
                TextButton(onClick = { confirmAll = false }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
}

private fun labelFor(id: String): Int = when (id) {
    "photos" -> R.string.storage_photos
    "videos" -> R.string.storage_videos
    "audio" -> R.string.storage_audio
    "temp" -> R.string.storage_temp
    "other" -> R.string.storage_other
    "gifs" -> R.string.storage_gifs
    "links" -> R.string.storage_links
    else -> R.string.storage_other
}

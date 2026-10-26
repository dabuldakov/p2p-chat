package com.p2pchat.app

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.Storage
import androidx.compose.material3.Badge
import androidx.compose.material3.Card
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp

@Composable
internal fun SettingsVersionCard(
    version: String,
    update: AppUpdate?,
    updateCheckFailed: Boolean,
    onClick: () -> Unit,
) {
    Card {
        Row(
            modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(stringResource(R.string.version), style = MaterialTheme.typography.titleSmall)
            Spacer(Modifier.weight(1f))
            Text(
                "v$version",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (update != null) {
                Spacer(Modifier.width(8.dp))
                Text(
                    "→ ${update.version}",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.primary,
                )
                Spacer(Modifier.width(8.dp))
                Badge(containerColor = Color.Red)
            }
        }
        if (updateCheckFailed) {
            Text(
                stringResource(R.string.update_check_failed),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 12.dp),
            )
        }
    }
}

@Composable
internal fun SettingsStorageCard(sizeText: String, onClick: () -> Unit) {
    Card {
        Row(
            modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Filled.Storage, contentDescription = null)
            Spacer(Modifier.width(10.dp))
            Text(stringResource(R.string.storage_title), style = MaterialTheme.typography.titleSmall)
            Spacer(Modifier.weight(1f))
            Text(
                sizeText,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun SettingsNicknameCard(
    nickname: String,
    editing: Boolean,
    draft: String,
    onDraftChange: (String) -> Unit,
    onEdit: () -> Unit,
    onSave: () -> Unit,
) {
    Card {
        Column(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.nickname), style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.weight(1f))
                if (editing) {
                    TextButton(onClick = onSave) { Text(stringResource(R.string.save)) }
                } else {
                    TextButton(onClick = onEdit) { Text(stringResource(R.string.change)) }
                }
            }
            if (editing) {
                OutlinedTextField(
                    value = draft,
                    onValueChange = onDraftChange,
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = { Text(stringResource(R.string.nickname_hint)) },
                    singleLine = true,
                )
            } else {
                Text(
                    nickname.ifBlank { stringResource(R.string.nickname_none) },
                    style = MaterialTheme.typography.bodyLarge,
                )
            }
        }
    }
}

@Composable
internal fun SettingsAvatarCard(
    avatar: ByteArray?,
    label: String,
    peerId: String,
    onChange: () -> Unit,
) {
    Card {
        Row(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Avatar(avatar, peerId, 48.dp)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.avatar), style = MaterialTheme.typography.titleSmall)
                Text(
                    label,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            TextButton(onClick = onChange) { Text(stringResource(R.string.change)) }
        }
    }
}

@Composable
internal fun SettingsBackupCard(
    fingerprint: String,
    onShowSeed: () -> Unit,
    onRestore: () -> Unit,
) {
    Card {
        Column(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(stringResource(R.string.backup_title), style = MaterialTheme.typography.titleSmall)
            Text(
                stringResource(R.string.backup_desc),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (fingerprint.isNotBlank()) {
                Text(
                    stringResource(R.string.fingerprint_label) + ": " + fingerprint,
                    style = MaterialTheme.typography.bodySmall,
                    fontFamily = FontFamily.Monospace,
                )
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = onShowSeed) { Text(stringResource(R.string.show_seed)) }
                TextButton(onClick = onRestore) { Text(stringResource(R.string.restore_seed)) }
            }
        }
    }
}

@Composable
internal fun SettingsLinkCard(title: String, onClick: () -> Unit) {
    Card {
        Row(
            modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(title, style = MaterialTheme.typography.titleSmall)
            Spacer(Modifier.weight(1f))
            Text(
                "↗",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
internal fun SettingsBlockedCard(
    blocked: Set<String>,
    nicknames: Map<String, String>,
    onUnblock: (String) -> Unit,
) {
    Card {
        Column(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(stringResource(R.string.blocked_users), style = MaterialTheme.typography.titleSmall)
            blocked.forEach { hex ->
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        nicknames[hex]?.takeIf { it.isNotBlank() } ?: hex.take(10),
                        style = MaterialTheme.typography.bodySmall,
                    )
                    Spacer(Modifier.weight(1f))
                    TextButton(onClick = { onUnblock(hex) }) {
                        Text(stringResource(R.string.unblock))
                    }
                }
            }
        }
    }
}

@Composable
internal fun SettingsToggleCard(
    title: String,
    description: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
) {
    Card {
        Row(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.titleSmall)
                Text(
                    description,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Spacer(Modifier.width(8.dp))
            Switch(checked = checked, onCheckedChange = onChange)
        }
    }
}

@Composable
internal fun SettingsLanguageCard(label: String, onClick: () -> Unit) {
    Card {
        Row(
            modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(stringResource(R.string.language), style = MaterialTheme.typography.titleSmall)
            Spacer(Modifier.weight(1f))
            Text(label, style = MaterialTheme.typography.bodyLarge)
        }
    }
}

@Composable
internal fun SettingsConnectionCard(
    direct: Int,
    turn: Int,
    lastSignaling: Long,
    explainKind: String?,
    onExplain: (String) -> Unit,
) {
    Card {
        Column(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(stringResource(R.string.connection), style = MaterialTheme.typography.titleSmall)
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    stringResource(R.string.connection_type),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.weight(1f))
                ConnTypeChip("STUN $direct", active = direct > 0, color = Color(0xFF2E9E6B)) {
                    onExplain("stun")
                }
                Spacer(Modifier.width(10.dp))
                ConnTypeChip("TURN $turn", active = turn > 0, color = Color(0xFFE08A1E)) {
                    onExplain("turn")
                }
            }
            explainKind?.let { ConnExplanation(it) }
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    stringResource(R.string.connection_last_signaling),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.weight(1f))
                Text(fmtTime(lastSignaling), style = MaterialTheme.typography.bodyMedium)
            }
        }
    }
}

@Composable
internal fun SettingsGraphCard(onClick: () -> Unit) {
    Card {
        Row(
            modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Filled.Hub, contentDescription = null)
            Spacer(Modifier.width(12.dp))
            Text(stringResource(R.string.graph_all_groups), style = MaterialTheme.typography.titleSmall)
        }
    }
}

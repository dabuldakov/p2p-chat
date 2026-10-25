package com.p2pchat.app

import android.content.SharedPreferences
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import uniffi.p2pchat_ffi.FfiMessage

private const val RECENT_KEY = "recent_reactions"
private const val RECENT_LIMIT = 10

private data class ReactionCategory(val title: Int, val emojis: List<String>)

private val CATEGORIES = listOf(
    ReactionCategory(R.string.reaction_cat_smileys, listOf(
        "😀","😃","😄","😁","😆","😅","😂","🤣","😊","😇","🙂","🙃","😉","😌","😍","🥰","😘","😗","😙","😚","😋","😛","😝","😜","🤪","🤨","🧐","🤓","😎","🥳","😏","😒","😞","😔","😟","😕","🙁","☹️","😣","😖","😫","😩","🥺","😢","😭","😤","😠","😡","🤬","🤯","😳","🥵","🥶","😱","😨","😰","😥","😓","🤗","🤔","🤭","🤫","🤥","😶","😐","😑","😬","🙄","😯","😦","😧","😮","😲","🥱","😴","🤤","😪","😵","🤐","🥴","🤢","🤮","🤧","😷","🤒","🤕"
    )),
    ReactionCategory(R.string.reaction_cat_gestures, listOf(
        "👍","👎","👌","✌️","🤞","🤟","🤘","🤙","👈","👉","👆","👇","☝️","✋","🤚","🖐️","🖖","👋","🤝","🙏","💪","🦾","✊","👊","🤛","🤜","👏","🙌","👐","🤲"
    )),
    ReactionCategory(R.string.reaction_cat_hearts, listOf(
        "❤️","🧡","💛","💚","💙","💜","🖤","🤍","🤎","💔","❣️","💕","💞","💓","💗","💖","💘","💝","💟","♥️"
    )),
    ReactionCategory(R.string.reaction_cat_animals, listOf(
        "🐶","🐱","🐭","🐹","🐰","🦊","🐻","🐼","🐨","🐯","🦁","🐮","🐷","🐸","🐵","🐔","🐧","🐦","🐤","🦆","🦅","🦉","🦇","🐺","🐗","🐴","🦄","🐝","🐛","🦋","🐌","🐞","🐢","🐍","🦎","🐙","🦑","🦀","🐠","🐟","🐬","🐳","🦈","🐊","🐅","🐆","🦓","🦍","🐘","🦏","🐪","🐫","🦒","🦘","🐄","🐎","🐖","🐑","🐐","🦌"
    )),
    ReactionCategory(R.string.reaction_cat_food, listOf(
        "🍏","🍎","🍐","🍊","🍋","🍌","🍉","🍇","🍓","🫐","🍈","🍒","🍑","🥭","🍍","🥥","🥝","🍅","🍆","🥑","🥦","🥬","🥒","🌶️","🫑","🌽","🥕","🫒","🧄","🧅","🥔","🍠","🥐","🥯","🍞","🥖","🥨","🧇","🥞","🧈","🍳","🧀","🥓","🥩","🍗","🍖","🌭","🍔","🍟","🍕","🥪","🥙","🧆","🌮","🌯","🥗","🥘","🍝","🍜","🍲","🍛","🍣","🍱","🥟","🍤","🍙","🍚","🍘","🍥","🥠","🥮","🍢","🍡","🍧","🍨","🍦","🥧","🧁","🍰","🎂","🍮","🍭","🍬","🍫","🍿","🍩","🍪","🌰","🥜","🍯","🥛","🍼","☕","🍵","🧃","🥤","🍶","🍺","🍻","🥂","🍷","🥃","🍸","🍹","🧉","🍾"
    )),
    ReactionCategory(R.string.reaction_cat_other, listOf(
        "🔥","✨","🎉","🎊","💯","✅","❌","⚡","💥","💫","💦","💨","⏰","⏳","⌛","💤","🎵","🎶","📌","📍","🚀","🎯","🏆","🥇","🥈","🥉","⚽","🏀","🎾","🎮","🎲","🎁","🎈","🎃","🎄","🎅","💡","💎","🔔","🔕","📣","📢","💬","💭","👀","🧠","🫶"
    )),
)

private fun recentFrom(prefs: SharedPreferences): List<String> =
    prefs.getString(RECENT_KEY, "")
        ?.split('|')
        ?.filter { it.isNotBlank() }
        ?.take(RECENT_LIMIT)
        .orEmpty()

private fun pushRecent(prefs: SharedPreferences, emoji: String): List<String> {
    val next = (listOf(emoji) + recentFrom(prefs).filter { it != emoji }).take(RECENT_LIMIT)
    prefs.edit().putString(RECENT_KEY, next.joinToString("|")).apply()
    return next
}

@Composable
fun ReactionPickerDialog(
    vm: ChatViewModel,
    message: FfiMessage,
    onDismiss: () -> Unit,
) {
    val ctx = LocalContext.current
    val prefs = remember { ctx.getSharedPreferences("p2pchat", 0) }
    var recent by remember { mutableStateOf(recentFrom(prefs)) }
    var selected by remember(message.id) {
        mutableStateOf(message.reactions.filter { it.mine }.map { it.emoji }.toSet())
    }

    fun choose(emoji: String) {
        val add = emoji !in selected
        if (vm.react(message.id, emoji, add)) {
            selected = if (add) selected + emoji else selected - emoji
            if (add) recent = pushRecent(prefs, emoji)
        }
    }

    Dialog(onDismissRequest = onDismiss) {
        Surface(
            shape = RoundedCornerShape(18.dp),
            color = MaterialTheme.colorScheme.surface,
            tonalElevation = 8.dp,
            modifier = Modifier.fillMaxWidth(0.94f).heightIn(max = 540.dp),
        ) {
            Column(Modifier.padding(12.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        stringResource(R.string.reaction),
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Spacer(Modifier.weight(1f))
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.close))
                    }
                }
                if (recent.isNotEmpty()) {
                    Text(
                        stringResource(R.string.reaction_recent),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Row(
                        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(vertical = 4.dp),
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        recent.forEach { e -> EmojiButton(e, e in selected) { choose(e) } }
                    }
                    HorizontalDivider(Modifier.padding(vertical = 6.dp))
                }
                Column(
                    Modifier
                        .weight(1f, fill = false)
                        .verticalScroll(rememberScrollState()),
                ) {
                    CATEGORIES.forEach { cat ->
                        Text(
                            stringResource(cat.title),
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(top = 8.dp, bottom = 2.dp),
                        )
                        cat.emojis.chunked(7).forEach { row ->
                            Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                                row.forEach { e -> EmojiButton(e, e in selected) { choose(e) } }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun EmojiButton(emoji: String, selected: Boolean, onClick: () -> Unit) {
    Box(
        modifier = Modifier
            .size(40.dp)
            .clip(CircleShape)
            .background(if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface)
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(emoji, fontSize = 25.sp)
    }
}

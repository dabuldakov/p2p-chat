package com.p2pchat.app.gif

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.grid.rememberLazyGridState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.compose.AsyncImage
import com.p2pchat.app.R
import com.p2pchat.app.GifProvider
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.distinctUntilChanged

private const val PAGE_SIZE = 30

enum class GifTab(val labelRes: Int) {
    MINE(R.string.gif_mine),
    GROUP(R.string.gif_group),
    SEARCH(R.string.gif_search),
}

/**
 * Пикер гифок: «Мои» / «Эта группа» (из локальной истории) и «Поиск» (KLIPY).
 * Тап по гифке отправляет её (в сообщении едет только ссылка kind+id).
 */
@Composable
fun GifPickerDialog(provider: GifProvider, onDismiss: () -> Unit) {
    var tab by remember { mutableStateOf(GifTab.MINE) }
    var query by remember { mutableStateOf("") }
    var gifs by remember { mutableStateOf<List<Gif>>(emptyList()) }
    var loading by remember { mutableStateOf(false) }
    var endReached by remember { mutableStateOf(false) }
    var page by remember { mutableStateOf(0) }
    val gridState = rememberLazyGridState()

    // Перезагрузка при смене вкладки/запроса.
    LaunchedEffect(tab, query) {
        gifs = emptyList()
        endReached = false
        page = 0
        loading = true
        when (tab) {
            GifTab.MINE -> {
                gifs = resolveRefs(provider, provider.myGifRefs())
                endReached = true
            }
            GifTab.GROUP -> {
                gifs = resolveRefs(provider, provider.groupGifRefs())
                endReached = true
            }
            GifTab.SEARCH -> {
                delay(300) // debounce ввода
                val res = provider.searchGifs(query, 0)
                gifs = res
                page = 1
                endReached = res.size < PAGE_SIZE
            }
        }
        loading = false
    }

    // Догрузка страниц поиска у нижней границы.
    LaunchedEffect(gridState, tab) {
        if (tab != GifTab.SEARCH) return@LaunchedEffect
        snapshotFlow { gridState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: -1 }
            .distinctUntilChanged()
            .collect { last ->
                if (!loading && !endReached && last >= gifs.size - 6) {
                    loading = true
                    val next = provider.searchGifs(query, page)
                    gifs = gifs + next
                    page += 1
                    endReached = next.size < PAGE_SIZE
                    loading = false
                }
            }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            TabRow(selectedTabIndex = tab.ordinal) {
                GifTab.entries.forEach { t ->
                    Tab(
                        selected = tab == t,
                        onClick = { if (tab != t) { tab = t; query = "" } },
                        text = { Text(stringResource(t.labelRes)) },
                    )
                }
            }
        },
        text = {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                if (tab == GifTab.SEARCH) {
                    OutlinedTextField(
                        value = query,
                        onValueChange = { query = it },
                        modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                        placeholder = { Text(stringResource(R.string.gif_search_hint)) },
                        singleLine = true,
                    )
                }
                Box(Modifier.fillMaxWidth().height(380.dp)) {
                    when {
                        gifs.isEmpty() && loading -> CircularProgressIndicator(Modifier.align(Alignment.Center))
                        gifs.isEmpty() -> Text(
                            if (tab == GifTab.SEARCH && query.isBlank()) stringResource(R.string.empty) else stringResource(R.string.nothing_found),
                            modifier = Modifier.align(Alignment.Center),
                        )
                        else -> LazyVerticalGrid(
                            state = gridState,
                            columns = GridCells.Fixed(3),
                            contentPadding = PaddingValues(2.dp),
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                            verticalArrangement = Arrangement.spacedBy(4.dp),
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            items(gifs, key = { it.providerId }) { gif ->
                                GifCell(gif) { provider.sendGif(gif); onDismiss() }
                            }
                            if (loading) {
                                item {
                                    Box(Modifier.size(64.dp), contentAlignment = Alignment.Center) {
                                        CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                                    }
                                }
                            }
                        }
                    }
                }
                // Атрибуция KLIPY — только на вкладке поиска, аккуратной подписью внизу.
                if (tab == GifTab.SEARCH) {
                    Text(
                        stringResource(R.string.gif_powered_by),
                        modifier = Modifier.fillMaxWidth().padding(6.dp),
                        textAlign = TextAlign.Center,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        fontSize = 11.sp,
                    )
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(stringResource(R.string.close)) }
        },
    )
}

private suspend fun resolveRefs(provider: GifProvider, refs: List<GifRef>): List<Gif> =
    refs.mapNotNull { provider.cachedGif(it.providerId) ?: provider.resolveGif(it.providerId) }

@Composable
private fun GifCell(gif: Gif, onClick: () -> Unit) {
    val w = gif.width ?: 0
    val h = gif.height ?: 0
    val aspect = if (w > 0 && h > 0) w.toFloat() / h.toFloat() else 1f
    AsyncImage(
        model = gif.previewUrl ?: gif.url,
        contentDescription = gif.title,
        contentScale = ContentScale.Crop,
        modifier = Modifier
            .aspectRatio(aspect)
            .clip(RoundedCornerShape(8.dp))
            .clickable(onClick = onClick),
    )
}

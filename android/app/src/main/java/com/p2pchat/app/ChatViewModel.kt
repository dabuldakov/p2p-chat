package com.p2pchat.app

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import com.p2pchat.app.gif.Gif
import com.p2pchat.app.gif.GifRef
import uniffi.p2pchat_ffi.FfiLink

/**
 * Тонкая обёртка над [ChatRuntime] для Compose. Вся логика и состояние —
 * в общепроцессном рантайме (чтобы работать и при закрытом экране).
 */
class ChatViewModel(app: Application) : AndroidViewModel(app) {

    init {
        ChatRuntime.init(app)
    }

    val publicKey get() = ChatRuntime.publicKey
    val publicKeyHex get() = ChatRuntime.publicKeyHex
    val peerId get() = ChatRuntime.peerId
    val version get() = ChatRuntime.version

    val connected get() = ChatRuntime.connected
    val groups get() = ChatRuntime.groups
    val unread get() = ChatRuntime.unread
    val memberCounts get() = ChatRuntime.memberCounts
    val currentGroup get() = ChatRuntime.currentGroup
    val messages get() = ChatRuntime.messages
    val hasMore get() = ChatRuntime.hasMore
    val members get() = ChatRuntime.members
    val status get() = ChatRuntime.status
    val invite get() = ChatRuntime.invite
    val isOwner get() = ChatRuntime.isOwner
    val update get() = ChatRuntime.update
    val sharedDraft get() = ChatRuntime.sharedDraft
    val backgroundEnabled get() = ChatRuntime.backgroundEnabled
    val debugLogsEnabled get() = ChatRuntime.debugLogsEnabled
    val peerKinds get() = ChatRuntime.peerKinds
    val directCount get() = ChatRuntime.directCount
    val turnCount get() = ChatRuntime.turnCount
    val lastSignalingTs get() = ChatRuntime.lastSignalingTs
    val nicknames get() = ChatRuntime.nicknames
    val myNickname get() = ChatRuntime.myNickname
    val avatars get() = ChatRuntime.avatars
    val groupAvatar get() = ChatRuntime.groupAvatarBytes
    val myAvatar get() = ChatRuntime.avatars[ChatRuntime.peerId]

    fun createGroup(name: String) = ChatRuntime.createGroup(name)
    fun joinGroup(invite: String) = ChatRuntime.joinGroup(invite)
    fun selectGroup(g: ByteArray) = ChatRuntime.selectGroup(g)
    fun leaveGroup() = ChatRuntime.leaveGroup()
    fun removeMember(peerId: ByteArray) = ChatRuntime.removeMember(peerId)
    fun backToList() = ChatRuntime.backToList()
    fun onSharedText(text: String) = ChatRuntime.onSharedText(text)
    fun consumeSharedDraft() = ChatRuntime.consumeSharedDraft()
    fun onForeground() = ChatRuntime.onForeground()
    fun setVisible(v: Boolean) = ChatRuntime.setVisible(v)
    fun links(group: ByteArray): List<FfiLink> = ChatRuntime.graphLinks(group)
    fun publishLinks() = ChatRuntime.publishLinks()
    fun graphEdges(group: ByteArray): List<GraphEdge> = ChatRuntime.graphEdges(group)
    fun send(text: String) = ChatRuntime.send(text)
    fun loadOlder() = ChatRuntime.loadOlder()
    fun refresh() = ChatRuntime.refresh()
    fun checkNow(onDone: (Boolean) -> Unit) = ChatRuntime.checkNow(onDone)
    fun setBackground(enabled: Boolean) = ChatRuntime.setBackground(enabled)
    fun setDebugLogs(enabled: Boolean) = ChatRuntime.setDebugLogs(enabled)

    // --- Гифки -----------------------------------------------------------
    val gifEnabled get() = ChatRuntime.gifEnabled
    fun sendGif(gif: Gif) = ChatRuntime.sendGif(gif)
    suspend fun searchGifs(query: String, page: Int): List<Gif> = ChatRuntime.searchGifs(query, page)
    suspend fun resolveGif(providerId: String): Gif? = ChatRuntime.resolveGif(providerId)
    fun cachedGif(providerId: String): Gif? = ChatRuntime.cachedGif(providerId)
    fun myGifRefs(limit: Int = 60): List<GifRef> = ChatRuntime.myGifRefs(limit)
    fun groupGifRefs(limit: Int = 60): List<GifRef> = ChatRuntime.groupGifRefs(limit)
    fun setNickname(name: String) = ChatRuntime.setNickname(name)
    fun setAvatar(b: ByteArray) = ChatRuntime.setAvatar(b)
    fun setGroupAvatar(b: ByteArray) = ChatRuntime.setGroupAvatar(b)
    fun regenInvite(maxUses: Int): String? = ChatRuntime.regenInvite(maxUses)
}

internal fun ByteArray.toHex(): String = joinToString("") { "%02x".format(it) }

/** hex-строка → байты (null, если некорректна). */
internal fun hexToBytes(hex: String): ByteArray? {
    val clean = hex.trim().replace(" ", "")
    if (clean.isEmpty() || clean.length % 2 != 0) return null
    return runCatching {
        ByteArray(clean.length / 2) { i ->
            clean.substring(i * 2, i * 2 + 2).toInt(16).toByte()
        }
    }.getOrNull()
}
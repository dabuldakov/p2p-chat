package com.p2pchat.app

import android.app.Application
import androidx.lifecycle.AndroidViewModel

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
    val nicknames get() = ChatRuntime.nicknames
    val myNickname get() = ChatRuntime.myNickname

    fun createGroup(name: String) = ChatRuntime.createGroup(name)
    fun joinGroup(invite: String) = ChatRuntime.joinGroup(invite)
    fun selectGroup(g: ByteArray) = ChatRuntime.selectGroup(g)
    fun leaveGroup() = ChatRuntime.leaveGroup()
    fun removeMember(peerId: ByteArray) = ChatRuntime.removeMember(peerId)
    fun backToList() = ChatRuntime.backToList()
    fun onSharedText(text: String) = ChatRuntime.onSharedText(text)
    fun consumeSharedDraft() = ChatRuntime.consumeSharedDraft()
    fun onForeground() = ChatRuntime.onForeground()
    fun send(text: String) = ChatRuntime.send(text)
    fun loadOlder() = ChatRuntime.loadOlder()
    fun refresh() = ChatRuntime.refresh()
    fun checkNow(onDone: (Boolean) -> Unit) = ChatRuntime.checkNow(onDone)
    fun setBackground(enabled: Boolean) = ChatRuntime.setBackground(enabled)
    fun setDebugLogs(enabled: Boolean) = ChatRuntime.setDebugLogs(enabled)
    fun setNickname(name: String) = ChatRuntime.setNickname(name)
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
package com.p2pchat.app

import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiMember
import uniffi.p2pchat_ffi.FfiMessage

/** Чтение состояния групп/сообщений из Rust-ядра одним снимком. */
internal class SnapshotRepository(
    private val core: ChatCore,
    private val peerId: () -> String,
    private val pageSize: Int,
) {
    fun load(group: ByteArray?, appVisible: Boolean): Snapshot {
        val ids = core.listGroups()
        val blocked = core.blockedPeers().map { it.toHex() }.toSet()
        val infos = ids
            .map { GroupInfo(it, core.groupName(it)) }
            .sortedBy { it.name.lowercase() }
        val elections = ids.associateBy(
            { it.toHex() },
            { runCatching { core.ownerElection(it) }.getOrNull() },
        )
        val ownerFlags = ids.associate { it.toHex() to runCatching { core.isOwner(it) }.getOrDefault(false) }
        var currentName: String? = null
        var isOwner = false
        var ownerId: String? = null
        var members = emptyList<FfiMember>()
        var pageMessages = emptyList<FfiMessage>()
        var hasMore = false
        var ownerLastSeen = 0L
        var ownerEpoch = 0uL
        var ownerChallengeAt = 0uL
        var groupAvatar: ByteArray? = null
        var neighborCount = 0
        if (group != null) {
            currentName = core.groupName(group)
            isOwner = runCatching { core.isOwner(group) }.getOrDefault(false)
            ownerId = runCatching { core.ownerId(group)?.toHex() }.getOrNull()
            members = core.members(group)
            val page = core.historyWindow(group, null, pageSize.toUInt())
            pageMessages = page.messages
            hasMore = page.hasMore
            if (shouldMarkRead(appVisible, true)) runCatching { core.markRead(group) }
            ownerLastSeen = core.ownerLastSeen(group).toLong() * 1000L
            ownerEpoch = core.ownerEpoch(group)
            ownerChallengeAt = core.ownerChallengeAt(group)
            groupAvatar = core.groupAvatar(group)
            neighborCount = runCatching {
                core.neighborKeys(group).distinctBy { it.toHex() }.size
            }.getOrDefault(0)
        }
        val nicks = core.allNicknames().associate { it.peerId.toHex() to it.nickname }.toMutableMap()
        val myNick = core.myNickname()
        myNick?.takeIf { it.isNotBlank() }?.let { nicks[peerId()] = it }
        return Snapshot(
            group = group,
            ids = ids,
            blocked = blocked,
            groupInfos = infos,
            currentGroupName = currentName,
            isOwner = isOwner,
            ownerId = ownerId,
            members = members,
            messages = pageMessages,
            hasMore = hasMore,
            unread = ids.associate { it.toHex() to core.unreadCount(it) },
            memberCounts = ids.associate { it.toHex() to core.members(it).size },
            myNickname = myNick,
            nicknames = nicks,
            avatars = runCatching { core.avatars() }.getOrDefault(emptyList())
                .associate { it.peerId.toHex() to it.avatar },
            groupAvatars = ids.mapNotNull { g ->
                runCatching { core.groupAvatar(g) }.getOrNull()?.let { g.toHex() to it }
            }.toMap(),
            ownerLastSeen = ownerLastSeen,
            elections = elections,
            ownerEpoch = ownerEpoch,
            ownerChallengeAt = ownerChallengeAt,
            ownerFlags = ownerFlags,
            groupAvatarBytes = groupAvatar,
            neighborCount = neighborCount,
        )
    }
}

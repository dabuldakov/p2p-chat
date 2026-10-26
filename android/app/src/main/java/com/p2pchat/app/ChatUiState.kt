package com.p2pchat.app

import uniffi.p2pchat_ffi.FfiMember
import uniffi.p2pchat_ffi.FfiMessage
import uniffi.p2pchat_ffi.FfiOwnerElection

/** Минимальное неизменяемое состояние списка групп для Compose. */
data class ChatUiState(
    val groupInfos: List<GroupInfo> = emptyList(),
    val unread: Map<String, ULong> = emptyMap(),
    val memberCounts: Map<String, Int> = emptyMap(),
    val groupAvatars: Map<String, ByteArray> = emptyMap(),
    val currentGroup: ByteArray? = null,
    val currentGroupName: String? = null,
    val myPeerId: String = "",
    val isOwner: Boolean = false,
    val ownerId: String? = null,
    val members: List<FfiMember> = emptyList(),
    val messages: List<FfiMessage> = emptyList(),
    val hasMore: Boolean = false,
    val status: String = "",
    val nicknames: Map<String, String> = emptyMap(),
    val avatars: Map<String, ByteArray> = emptyMap(),
    val groupAvatar: ByteArray? = null,
    val peerKinds: Map<String, String> = emptyMap(),
    val connected: Int = 0,
    val groupConnected: Int = 0,
    val groupNeighbors: Int = 0,
    val ownerLastSeen: Long = 0L,
    val ownerElection: FfiOwnerElection? = null,
    val ownerChangeNotice: String? = null,
    val ownerChallengeAt: ULong = 0uL,
    val blocked: Set<String> = emptySet(),
    val inviteMaxUses: Int? = null,
    val inviteAt: Long? = null,
    val gifEnabled: Boolean = false,
    val sharedDraft: String? = null,
)

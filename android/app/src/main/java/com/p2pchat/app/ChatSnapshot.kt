package com.p2pchat.app

import com.p2pchat.app.gif.GifRef
import uniffi.p2pchat_ffi.FfiAttachment
import uniffi.p2pchat_ffi.FfiMember
import uniffi.p2pchat_ffi.FfiMessage
import uniffi.p2pchat_ffi.FfiOwnerElection

internal data class Snapshot(
    val group: ByteArray?,
    val ids: List<ByteArray>,
    val blocked: Set<String>,
    val groupInfos: List<GroupInfo>,
    val currentGroupName: String?,
    val isOwner: Boolean,
    val ownerId: String?,
    val members: List<FfiMember>,
    val messages: List<FfiMessage>,
    val hasMore: Boolean,
    val unread: Map<String, ULong>,
    val memberCounts: Map<String, Int>,
    val myNickname: String?,
    val nicknames: Map<String, String>,
    val avatars: Map<String, ByteArray>,
    val groupAvatars: Map<String, ByteArray>,
    val ownerLastSeen: Long,
    val elections: Map<String, FfiOwnerElection?>,
    val ownerEpoch: ULong,
    val ownerChallengeAt: ULong,
    val ownerFlags: Map<String, Boolean>,
    val groupAvatarBytes: ByteArray?,
    val neighborCount: Int,
)
internal fun sameGroup(a: ByteArray?, b: ByteArray?): Boolean =
    (a == null && b == null) || (a != null && b != null && a.contentEquals(b))

internal fun sameBytes(a: ByteArray?, b: ByteArray?): Boolean = sameGroup(a, b)

internal fun sameAvatars(a: Map<String, ByteArray>, b: Map<String, ByteArray>): Boolean {
    if (a.size != b.size) return false
    for ((k, v) in a) {
        val w = b[k] ?: return false
        if (!v.contentEquals(w)) return false
    }
    return true
}

internal fun toGifRef(a: FfiAttachment): GifRef = GifRef(
    providerId = a.id,
    width = a.width.toInt().takeIf { it > 0 },
    height = a.height.toInt().takeIf { it > 0 },
)

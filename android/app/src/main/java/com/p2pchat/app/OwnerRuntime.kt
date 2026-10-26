package com.p2pchat.app

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiOutgoing
import uniffi.p2pchat_ffi.FfiOwnerElection

/**
 * Выборы владельца: heartbeat, заявки, голоса и авто-претендент.
 * Состояние для UI остаётся в ChatRuntime, здесь — только оркестрация.
 */
internal class OwnerRuntime(
    private val core: ChatCore,
    private val scope: CoroutineScope,
    private val currentGroup: () -> ByteArray?,
    private val isOwner: () -> Boolean,
    private val ownerLastSeen: () -> Long,
    private val ownerElection: () -> FfiOwnerElection?,
    private val memberCount: () -> Int,
    private val send: (FfiOutgoing) -> Unit,
    private val refresh: () -> Unit,
    private val toast: (Int) -> Unit,
) {
    private val heartbeatAt = HashMap<String, Long>()
    private val autoAt = HashMap<String, Long>()

    fun heartbeat() {
        val group = currentGroup() ?: return
        if (!isOwner()) return
        val key = group.toHex()
        val now = System.currentTimeMillis()
        if (now - (heartbeatAt[key] ?: 0L) < 12 * 60 * 60_000L) return
        heartbeatAt[key] = now
        scope.launch {
            val out = withContext(Dispatchers.IO) {
                runCatching { core.ownerAlive(group) }.getOrNull()
            }
            out?.let(send)
        }
    }

    fun claim(trigger: Int) {
        val group = currentGroup() ?: return
        scope.launch {
            val out = withContext(Dispatchers.IO) {
                runCatching { core.createOwnerClaim(group, trigger.toUInt()) }.getOrNull()
            }
            if (out == null) {
                refresh()
                val msg = when {
                    ownerElection() != null -> R.string.owner_election_title
                    memberCount() <= 2 -> R.string.owner_two_people_hint
                    else -> R.string.owner_claim_unavailable
                }
                toast(msg)
                return@launch
            }
            send(out)
            refresh()
        }
    }

    fun vote(claimId: ByteArray, approve: Boolean, keep: Boolean) {
        val group = currentGroup() ?: return
        scope.launch {
            val out = withContext(Dispatchers.IO) {
                runCatching { core.ownerVote(group, claimId, approve, keep) }.getOrNull()
            }
            if (out == null) {
                toast(R.string.owner_vote_unavailable)
                return@launch
            }
            send(out)
            refresh()
            advance()
        }
    }

    fun keep() = vote(ByteArray(0), true, true)

    fun advance() {
        val group = currentGroup() ?: return
        val now = System.currentTimeMillis()
        val lastSeen = ownerLastSeen()
        val key = group.toHex()
        val shouldAuto = !isOwner() && lastSeen > 0L && now - lastSeen >= 8 * 86_400_000L &&
            now - (autoAt[key] ?: 0L) >= 10 * 60_000L
        if (shouldAuto) autoAt[key] = now
        scope.launch {
            val election = withContext(Dispatchers.IO) {
                runCatching { core.ownerElection(group) }.getOrNull()
            }
            if (election != null) {
                // Ядро само завершит выборы, когда все участники уже проголосовали
                // (для пары — сразу после голоса владельца), иначе no-op.
                val out = withContext(Dispatchers.IO) {
                    runCatching { core.finalizeOwnerElection(group) }.getOrNull()
                }
                out?.let(send)
                refresh()
                return@launch
            }
            if (!shouldAuto) return@launch
            val out = withContext(Dispatchers.IO) {
                runCatching { core.createOwnerClaim(group, 2u) }.getOrNull()
            }
            out?.let(send)
            refresh()
        }
    }
}

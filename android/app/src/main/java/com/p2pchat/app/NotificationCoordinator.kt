package com.p2pchat.app

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiOwnerElection

/** Бейджи групп и уведомления о выборах владельца. */
internal class NotificationCoordinator(
    private val app: Application,
    private val core: ChatCore,
    private val scope: CoroutineScope,
    private val groups: () -> List<GroupInfo>,
    private val unread: () -> Map<String, ULong>,
    private val appVisible: () -> Boolean,
    private val currentGroup: () -> ByteArray?,
    private val myPeerId: () -> String,
) {
    private val ownerNotified = HashMap<String, String>()

    fun updateBadge(silent: Boolean) {
        val snapshot = groups()
        val unreadNow = unread()
        val fallback = app.getString(R.string.new_messages)
        scope.launch {
            val items = withContext(Dispatchers.IO) {
                unreadGroups(snapshot, unreadNow).map { gi ->
                    val last = runCatching {
                        core.historyWindow(gi.id, null, 1u).messages.lastOrNull()?.text
                    }.getOrNull()
                    gi to notificationText(last, fallback)
                }
            }
            if (items.isEmpty()) Notifier.clearAll(app) else Notifier.showGroups(app, items, silent)
        }
    }

    fun maybeNotifyOwnerElection(group: GroupInfo, election: FfiOwnerElection?, iAmOwner: Boolean) {
        val key = group.id.toHex()
        if (election == null) {
            clear(key)
            return
        }
        if (appVisible() && currentGroup()?.contentEquals(group.id) == true) {
            clear(key)
            return
        }
        val peerId = myPeerId()
        val iAmClaimant = election.claims.any { it.claimant.toHex() == peerId }
        val iVoted = election.votes.any { it.voter.toHex() == peerId }
        val ownerNeedsVote = iAmOwner && election.trigger.toInt() == 1
        if (iAmClaimant || iVoted || (iAmOwner && !ownerNeedsVote)) {
            clear(key)
            return
        }
        val now = System.currentTimeMillis()
        val voteStarted = now >= election.voteStart.toLong() * 1000L
        val textRes = if (voteStarted) R.string.owner_notification_vote else R.string.owner_notification_start
        val sig = election.claims.joinToString(",") { it.claimId.toHex() } +
            ":" + election.trigger + ":" + voteStarted
        if (ownerNotified[key] == sig) return
        ownerNotified[key] = sig
        Notifier.showOwnerElection(app, key, group.name, app.getString(textRes))
    }

    fun clearMissing(knownGroups: Set<String>) {
        ownerNotified.keys.filter { it !in knownGroups }.forEach(::clear)
    }

    fun clear(key: String) {
        ownerNotified.remove(key)
        Notifier.clearOwnerElection(app, key)
    }
}

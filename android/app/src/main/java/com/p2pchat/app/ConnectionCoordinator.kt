package com.p2pchat.app

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.p2pchat_ffi.ChatCore
import uniffi.p2pchat_ffi.FfiOutgoing
import uniffi.p2pchat_ffi.FfiPeerLink

internal class ConnectionCoordinator(
    private val core: ChatCore,
    private val transport: WebRtcTransport,
    private val scope: CoroutineScope,
    private val turnCredsUrl: String,
    private val currentGroup: () -> ByteArray?,
    private val peerKinds: () -> Map<String, String>,
) {
    private val ownerReplayAt = HashMap<String, Long>()

    fun syncAll() {
        scope.launch {
            val (outs, replay) = withContext(Dispatchers.IO) {
                val result = mutableListOf<FfiOutgoing>()
                core.profileEnvelope()?.let { result.add(it) }
                val now = System.currentTimeMillis()
                val replayUpdates = mutableListOf<Pair<String, Long>>()
                for (g in core.listGroups()) {
                    for (key in core.neighborKeys(g)) {
                        runCatching { core.syncDigestTo(g, key) }.getOrNull()?.let { result.add(it) }
                    }
                    val gkey = g.toHex()
                    if (now - (ownerReplayAt[gkey] ?: 0L) > 60_000L) {
                        runCatching { core.ownerChangeEnvelope(g) }.getOrNull()?.let { result.add(it) }
                        replayUpdates.add(gkey to now)
                    }
                }
                result to replayUpdates
            }
            replay.forEach { (k, t) -> ownerReplayAt[k] = t }
            send(outs)
        }
    }

    fun onPeerConnected(peerKey: ByteArray) {
        scope.launch {
            val outs = withContext(Dispatchers.IO) {
                val result = mutableListOf<FfiOutgoing>()
                core.profileEnvelope()?.let { result.add(it) }
                for (g in core.groupsWith(peerKey)) {
                    runCatching { core.syncDigestTo(g, peerKey) }.getOrNull()?.let { result.add(it) }
                    runCatching { core.ownerChangeEnvelope(g) }.getOrNull()?.let { result.add(it) }
                }
                result
            }
            send(outs)
        }
    }

    fun publishLinks() {
        scope.launch {
            val outs = withContext(Dispatchers.IO) {
                val result = mutableListOf<FfiOutgoing>()
                for (g in core.listGroups()) {
                    val links = runCatching { core.members(g) }.getOrDefault(emptyList())
                        .mapNotNull { m ->
                            peerKinds()[m.publicKey.toHex()]?.let { FfiPeerLink(m.peerId, it) }
                        }
                    if (links.isEmpty()) continue
                    runCatching { core.postLinks(g, links) }.getOrNull()?.let { result.add(it) }
                }
                result
            }
            send(outs)
        }
    }

    fun connectAll() {
        scope.launch {
            val ice = withContext(Dispatchers.IO) { TurnCredentials.fetch(turnCredsUrl) }
            if (ice != null) transport.setIceServers(ice)
            val turnN = ice?.count { s -> s.urls.any { it.startsWith("turn") } } ?: 0
            Logs.i("ice: creds=${ice != null} servers=${ice?.size ?: 0} turn=$turnN")
            val keys = core.listGroups()
                .flatMap { core.neighborKeys(it) }
                .distinctBy { it.toHex() }
            val curCount = currentGroup()?.let { g ->
                runCatching { core.neighborKeys(g).distinctBy { it.toHex() }.size }.getOrDefault(0)
            } ?: keys.size
            Logs.d("connectAll: groups=${core.listGroups().size} peers=${keys.size} groupNeighbors=$curCount")
            keys.forEach { pk ->
                Logs.d("neighbor ${pk.toHex()}")
                runCatching { transport.connect(pk) }
            }
            Logs.d("group neighbors: $curCount")
        }
    }

    private fun send(outs: List<FfiOutgoing>) {
        outs.forEach { transport.sendTargets(it.envelope, it.targets) }
    }
}

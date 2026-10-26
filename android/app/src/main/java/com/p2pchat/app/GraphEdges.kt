package com.p2pchat.app

import uniffi.p2pchat_ffi.FfiLink
import uniffi.p2pchat_ffi.FfiMember

internal fun graphPairKey(a: String, b: String): String = if (a < b) "$a|$b" else "$b|$a"

internal fun buildGroupEdges(
    me: String,
    members: List<FfiMember>,
    links: List<FfiLink>,
    peerKinds: Map<String, String>,
): List<GraphEdge> {
    val out = LinkedHashMap<String, GraphEdge>()
    members.forEach { m ->
        val kind = peerKinds[m.publicKey.toHex()] ?: return@forEach
        val peer = m.peerId.toHex()
        out[graphPairKey(me, peer)] = GraphEdge(me, peer, kind)
    }
    links.forEach { l ->
        val a = l.from.toHex()
        val b = l.to.toHex()
        if (a == me || b == me) return@forEach
        out.putIfAbsent(graphPairKey(a, b), GraphEdge(a, b, l.kind))
    }
    return out.values.toList()
}

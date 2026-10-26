package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.p2pchat_ffi.FfiLink
import uniffi.p2pchat_ffi.FfiMember

class GraphEdgesTest {

    private fun member(peer: String, key: String = peer) =
        FfiMember(hexToBytes(peer)!!, hexToBytes(key)!!)

    private fun link(from: String, to: String, kind: String = "direct") =
        FfiLink(hexToBytes(from)!!, hexToBytes(to)!!, kind)

    @Test
    fun ownLiveEdgeWinsOverPublishedLink() {
        val edges = buildGroupEdges(
            me = "aa",
            members = listOf(member("bb")),
            links = listOf(link("aa", "bb", "turn")),
            peerKinds = mapOf("bb" to "direct"),
        )

        assertEquals(listOf(GraphEdge("aa", "bb", "direct")), edges)
    }

    @Test
    fun skippedUnknownOwnPeerAndDedupsForeignEdges() {
        val edges = buildGroupEdges(
            me = "aa",
            members = listOf(member("bb"), member("cc")),
            links = listOf(link("bb", "cc", "turn"), link("cc", "bb", "direct")),
            peerKinds = mapOf("bb" to "turn"),
        )

        assertEquals(
            listOf(GraphEdge("aa", "bb", "turn"), GraphEdge("bb", "cc", "turn")),
            edges,
        )
    }

    @Test
    fun ignoresLinksTouchingMeAndDuplicatesMembers() {
        val edges = buildGroupEdges(
            me = "aa",
            members = listOf(member("bb"), member("bb")),
            links = listOf(link("aa", "cc"), link("aa", "bb")),
            peerKinds = mapOf("bb" to "direct"),
        )

        assertEquals(listOf(GraphEdge("aa", "bb", "direct")), edges)
    }

    @Test
    fun usesPublicKeyForLiveKindButPeerIdForEdge() {
        val edges = buildGroupEdges(
            me = "aa",
            members = listOf(member(peer = "bb", key = "ff")),
            links = emptyList(),
            peerKinds = mapOf("ff" to "direct"),
        )

        assertEquals(listOf(GraphEdge("aa", "bb", "direct")), edges)
    }
}

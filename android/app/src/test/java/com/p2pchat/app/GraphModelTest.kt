package com.p2pchat.app

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.p2pchat_ffi.FfiMember

class GraphModelTest {

    private fun member(idHex: String) = FfiMember(hexToBytes(idHex)!!, ByteArray(1))

    @Test
    fun dedupsNodesByPeerIdAndIncludesEdgeEndpoints() {
        val a = member("aa")
        val members = listOf(a, a, member("bb")) // дубликат того же peer_id
        val edges = listOf(
            GraphEdge("aa", "bb", "direct"),
            GraphEdge("bb", "cc", ""), // endpoint cc нет в members
        )
        val model = buildGraphModel(members, edges, mapOf("aa" to "Ann"))

        assertEquals(3, model.nodes.size) // aa, bb, cc — без дублей
        assertEquals("Ann", model.nodes.first { it.id == "aa" }.label)
        assertEquals("bb", model.nodes.first { it.id == "bb" }.label)
        assertEquals(2, model.edges.size)
    }

    @Test
    fun duplicateEdgePrefersKnownKind() {
        val edges = listOf(
            GraphEdge("aa", "bb", ""),
            GraphEdge("bb", "aa", "turn"),
        )
        val model = buildGraphModel(emptyList(), edges, emptyMap())

        assertEquals(1, model.edges.size)
        assertEquals("turn", model.edges.single().kind)
        assertEquals(2, model.nodes.size)
    }

    @Test
    fun selfAppearsOnceWithNickname() {
        val model = buildGraphModel(
            listOf(member("aa")),
            listOf(GraphEdge("aa", "bb", "direct")),
            mapOf("aa" to "Me", "bb" to "Bob"),
        )
        assertEquals(listOf("aa", "bb"), model.nodes.map { it.id })
        assertEquals(listOf("Me", "Bob"), model.nodes.map { it.label })
    }
}

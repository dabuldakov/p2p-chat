package com.p2pchat.app

import uniffi.p2pchat_ffi.FfiMember

/** Узел графа: id (hex peer_id) и подпись. */
data class GraphNode(val id: String, val label: String)

/** Готовый граф: уникальные узлы и рёбра. */
data class GraphModel(val nodes: List<GraphNode>, val edges: List<GraphEdge>)

/**
 * Собрать граф из участников и рёбер:
 * - узлы дедуплицируются по peer_id (мой ник не превращается в отдельный узел);
 * - все концы рёбер тоже становятся узлами (не теряем связи);
 * - подпись — ник, иначе короткий id;
 * - рёбра дедуплицируются по паре; «direct»/«turn» приоритетнее неизвестного типа.
 */
fun buildGraphModel(
    members: List<FfiMember>,
    edges: List<GraphEdge>,
    nicknames: Map<String, String>,
): GraphModel {
    val nodes = LinkedHashMap<String, String>()

    fun label(id: String): String = nicknames[id]?.takeIf { it.isNotBlank() } ?: id.take(6)

    members.forEach { m ->
        val id = m.peerId.toHex()
        nodes.putIfAbsent(id, label(id))
    }
    edges.forEach { e ->
        nodes.putIfAbsent(e.from, label(e.from))
        nodes.putIfAbsent(e.to, label(e.to))
    }

    val unique = LinkedHashMap<String, GraphEdge>()
    edges.forEach { e ->
        val key = if (e.from < e.to) "${e.from}|${e.to}" else "${e.to}|${e.from}"
        val prev = unique[key]
        if (prev == null || (!isKnown(prev.kind) && isKnown(e.kind))) unique[key] = e
    }

    return GraphModel(
        nodes.map { GraphNode(it.key, it.value) },
        unique.values.toList(),
    )
}

private fun isKnown(kind: String): Boolean = kind == "direct" || kind == "turn"

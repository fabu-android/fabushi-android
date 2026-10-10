package com.ombhrum.fabushi

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

internal enum class AgentNetworkEdgeKind { MEMBERSHIP, MESSAGE }
internal enum class AgentNetworkEdgeActivity { IDLE, RECENT, TALKING }
internal enum class AgentNetworkActivity { IDLE, WORKING, WAITING }

internal data class AgentNetworkNode(
    val id: String,
    val name: String,
    val description: String,
    val isGroup: Boolean,
    val memberIds: List<String>,
    val conversationPartnerIds: List<String>,
    val awaitingUserResponse: Boolean?,
    val isRunning: Boolean,
    val lastMessage: String,
    val updatedAt: Long,
)

internal data class AgentNetworkEdge(
    val key: String,
    val sourceId: String,
    val targetId: String,
    val kind: AgentNetworkEdgeKind,
)

internal data class AgentNetworkPoint(val x: Float, val y: Float)
internal data class AgentNetworkViewport(val scale: Float = 1f, val x: Float = 0f, val y: Float = 0f)

internal fun MobileBotSummaryAndroid.toAgentNetworkNode() = AgentNetworkNode(
    id = id,
    name = name,
    description = description,
    isGroup = isGroup,
    memberIds = memberIds,
    conversationPartnerIds = conversationPartnerIds,
    awaitingUserResponse = awaitingUserResponse,
    isRunning = isRunning,
    lastMessage = lastMessage,
    updatedAt = updatedAt,
)

internal fun buildAgentNetworkEdges(nodes: List<AgentNetworkNode>): List<AgentNetworkEdge> {
    val known = nodes.mapTo(linkedSetOf()) { it.id }
    val emitted = linkedSetOf<String>()
    return buildList {
        nodes.forEach { node ->
            if (node.isGroup) {
                node.memberIds.forEach { memberId ->
                    if (memberId == node.id || memberId !in known) return@forEach
                    val key = "member::${node.id}::$memberId"
                    if (emitted.add(key)) add(AgentNetworkEdge(key, node.id, memberId, AgentNetworkEdgeKind.MEMBERSHIP))
                }
            } else {
                node.conversationPartnerIds.forEach { partnerId ->
                    if (partnerId == node.id || partnerId !in known) return@forEach
                    val source = minOf(node.id, partnerId)
                    val target = maxOf(node.id, partnerId)
                    val key = "msg::$source::$target"
                    if (emitted.add(key)) add(AgentNetworkEdge(key, source, target, AgentNetworkEdgeKind.MESSAGE))
                }
            }
        }
    }
}

internal fun agentNetworkEdgeActivity(edge: AgentNetworkEdge, nodesById: Map<String, AgentNetworkNode>, now: Long): AgentNetworkEdgeActivity {
    val source = nodesById[edge.sourceId] ?: return AgentNetworkEdgeActivity.IDLE
    val target = nodesById[edge.targetId] ?: return AgentNetworkEdgeActivity.IDLE
    if (
        source.isRunning && source.awaitingUserResponse == null &&
        target.isRunning && target.awaitingUserResponse == null
    ) return AgentNetworkEdgeActivity.TALKING
    return if (now - minOf(source.updatedAt, target.updatedAt) <= 120_000L) AgentNetworkEdgeActivity.RECENT else AgentNetworkEdgeActivity.IDLE
}

internal fun agentNetworkActivity(node: AgentNetworkNode): AgentNetworkActivity = when {
    node.awaitingUserResponse != null -> AgentNetworkActivity.WAITING
    node.isRunning -> AgentNetworkActivity.WORKING
    else -> AgentNetworkActivity.IDLE
}

internal fun agentNetworkSummary(nodes: List<AgentNetworkNode>, edges: List<AgentNetworkEdge>): String {
    val groups = nodes.count { it.isGroup }
    val agents = nodes.size - groups
    val messageLinks = edges.count { it.kind == AgentNetworkEdgeKind.MESSAGE }
    return "$agents ${if (agents == 1) "agent" else "agents"} · " +
        "$groups ${if (groups == 1) "group" else "groups"} · " +
        "$messageLinks message ${if (messageLinks == 1) "link" else "links"}"
}

internal fun layoutAgentNetwork(nodes: List<AgentNetworkNode>, width: Float, height: Float): Map<String, AgentNetworkPoint> {
    if (nodes.isEmpty()) return emptyMap()
    val safeWidth = max(width, 1f)
    val safeHeight = max(height, 1f)
    val centerX = safeWidth / 2f
    val centerY = safeHeight / 2f
    if (nodes.size == 1) return mapOf(nodes.first().id to AgentNetworkPoint(centerX, centerY))
    val sorted = nodes.sortedWith(compareByDescending<AgentNetworkNode> { it.isGroup }.thenBy { it.id })
    val goldenAngle = PI * (3.0 - sqrt(5.0))
    val maxRadius = min(safeWidth, safeHeight) * 0.36f
    return sorted.mapIndexed { index, node ->
        val fraction = sqrt((index + 1).toDouble() / sorted.size.toDouble()).toFloat()
        val radius = maxRadius * fraction
        val angle = index * goldenAngle
        node.id to AgentNetworkPoint(
            x = centerX + (cos(angle) * radius).toFloat(),
            y = centerY + (sin(angle) * radius).toFloat(),
        )
    }.toMap()
}

internal fun normalizeAgentNetworkViewport(viewport: AgentNetworkViewport, width: Float, height: Float, overscroll: Float = 0.5f): AgentNetworkViewport {
    val scale = viewport.scale.coerceIn(1f, 3f)
    val extraX = width * overscroll
    val extraY = height * overscroll
    return AgentNetworkViewport(
        scale = scale,
        x = viewport.x.coerceIn(width * (1f - scale) - extraX, extraX),
        y = viewport.y.coerceIn(height * (1f - scale) - extraY, extraY),
    )
}

internal fun transformAgentNetworkViewport(current: AgentNetworkViewport, zoom: Float, panX: Float, panY: Float, width: Float, height: Float): AgentNetworkViewport =
    normalizeAgentNetworkViewport(AgentNetworkViewport(current.scale * zoom, current.x + panX, current.y + panY), width, height)

internal fun reconcileAgentNetworkSelection(selectedId: String?, nodes: List<AgentNetworkNode>): String? =
    selectedId?.takeIf { id -> nodes.any { it.id == id } }

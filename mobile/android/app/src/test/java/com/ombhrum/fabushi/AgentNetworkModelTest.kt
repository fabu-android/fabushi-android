package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentNetworkModelTest {
    private fun node(
        id: String,
        isGroup: Boolean = false,
        members: List<String> = emptyList(),
        partners: List<String> = emptyList(),
        running: Boolean = false,
        waiting: Boolean = false,
        updatedAt: Long = 0L,
    ) = AgentNetworkNode(
        id = id,
        name = id,
        description = "",
        isGroup = isGroup,
        memberIds = members,
        conversationPartnerIds = partners,
        awaitingUserResponse = waiting,
        isRunning = running,
        lastMessage = "",
        updatedAt = updatedAt,
    )

    @Test fun edgesDeduplicateMessagePartnersAndRejectUnknownMembership() {
        val nodes = listOf(
            node("a", partners = listOf("b")),
            node("b", partners = listOf("a")),
            node("group", isGroup = true, members = listOf("a", "missing", "group")),
        )
        val edges = buildAgentNetworkEdges(nodes)
        assertEquals(2, edges.size)
        assertEquals(1, edges.count { it.kind == AgentNetworkEdgeKind.MESSAGE })
        assertEquals(1, edges.count { it.kind == AgentNetworkEdgeKind.MEMBERSHIP })
        assertTrue(edges.any { it.key == "msg::a::b" })
        assertTrue(edges.any { it.key == "member::group::a" })
    }

    @Test fun activityAndSummaryMatchDesktopSemantics() {
        val nodes = listOf(
            node("a", partners = listOf("b"), running = true, updatedAt = 100_000),
            node("b", partners = listOf("a"), running = true, updatedAt = 100_000),
            node("group", isGroup = true, members = listOf("a", "b"), waiting = true),
        )
        val edges = buildAgentNetworkEdges(nodes)
        val message = edges.single { it.kind == AgentNetworkEdgeKind.MESSAGE }
        assertEquals(AgentNetworkEdgeActivity.TALKING, agentNetworkEdgeActivity(message, nodes.associateBy { it.id }, 110_000))
        assertEquals(AgentNetworkActivity.WAITING, agentNetworkActivity(nodes.last()))
        assertEquals("2 agents · 1 group · 1 message link", agentNetworkSummary(nodes, edges))
    }

    @Test fun layoutAndViewportAreResponsiveBoundedAndDeterministic() {
        val nodes = listOf(node("b"), node("a"), node("c"))
        val first = layoutAgentNetwork(nodes, 900f, 600f)
        val second = layoutAgentNetwork(nodes.reversed(), 900f, 600f)
        assertEquals(first, second)
        assertTrue(first.values.all { it.x in 0f..900f && it.y in 0f..600f })
        val transformed = transformAgentNetworkViewport(
            AgentNetworkViewport(),
            zoom = 9f,
            panX = 10_000f,
            panY = -10_000f,
            width = 900f,
            height = 600f,
        )
        assertEquals(3f, transformed.scale)
        assertTrue(transformed.x <= 450f)
        assertTrue(transformed.y >= -1500f)
    }

    @Test fun staleSelectionIsReconciledWhenCanonicalRosterChanges() {
        val nodes = listOf(node("a"), node("b"))
        assertEquals("a", reconcileAgentNetworkSelection("a", nodes))
        assertNull(reconcileAgentNetworkSelection("gone", nodes))
    }
}

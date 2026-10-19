package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class SidebarModelTest {
    private data class Agent(
        override val id: String,
        override val isPinned: Boolean,
    ) : SidebarOrderAgent

    @Test
    fun partitionPreservesStoredPinnedOrderThenAppendsMissingPinnedAgents() {
        val agents = listOf(
            Agent("a", true),
            Agent("b", false),
            Agent("c", true),
            Agent("d", true),
        )
        val result = partitionSidebarAgents(
            agents = agents,
            pinnedAgentIds = listOf("c", "missing", "c", "a"),
        )

        assertEquals(listOf("c", "a", "d"), result.pinned.map { it.id })
        assertEquals(listOf("b"), result.unpinned.map { it.id })
    }

    @Test
    fun projectionUsesHostPinnedOrderAndCanonicalRecentActivity() {
        data class RecentAgent(
            override val id: String,
            override val isPinned: Boolean,
            val updatedAt: Long,
        ) : SidebarOrderAgent

        val projection = projectSidebarAgents(
            agents = listOf(
                RecentAgent("old", false, 10L),
                RecentAgent("pin-a", true, 40L),
                RecentAgent("new", false, 30L),
                RecentAgent("pin-b", true, 20L),
            ),
            pinnedAgentIds = listOf("pin-b", "pin-a"),
            updatedAt = RecentAgent::updatedAt,
        )

        assertEquals(listOf("pin-b", "pin-a"), projection.pinned.map { it.id })
        assertEquals(listOf("new", "old"), projection.recent.map { it.id })
    }

    @Test
    fun movePinnedAgentMatchesDesktopBeforeAfterAndNoopContracts() {
        val stored = listOf("a", "b", "c")
        assertEquals(
            listOf("c", "a", "b"),
            movePinnedAgent(stored, movedId = "c", targetId = "a", position = PinnedMovePosition.BEFORE),
        )
        assertEquals(
            listOf("b", "c", "a"),
            movePinnedAgent(stored, movedId = "a", targetId = "c", position = PinnedMovePosition.AFTER),
        )
        assertEquals(stored, movePinnedAgent(stored, "b", "b", PinnedMovePosition.AFTER))
        assertEquals(stored, movePinnedAgent(stored, "a", "missing", PinnedMovePosition.BEFORE))
    }
}

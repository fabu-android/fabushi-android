package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSection
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AgentSidebarSectionsModelTest {
    private val sections = listOf(
        AndroidSidebarSection("section-a", "A", listOf("agent-1")),
        AndroidSidebarSection("section-b", "B", listOf("agent-2")),
        AndroidSidebarSection(AGENT_UNASSIGNED_SECTION_ID, "Unassigned", listOf("agent-3")),
    )

    @Test
    fun moveRemovesPreviousMembershipAndTargetsCanonicalSection() {
        val next = requireNotNull(moveAgentToSidebarSection(sections, "agent-1", "section-b"))
        assertEquals(emptyList<String>(), next.first { it.id == "section-a" }.agentIds)
        assertEquals(listOf("agent-2", "agent-1"), next.first { it.id == "section-b" }.agentIds)
        assertEquals(2, next.size)
    }

    @Test
    fun moveToUnassignedRemovesDurableMembershipWithoutPersistingSyntheticSection() {
        val next = requireNotNull(
            moveAgentToSidebarSection(sections, "agent-1", AGENT_UNASSIGNED_SECTION_ID),
        )
        assertEquals(emptyList<String>(), next.first { it.id == "section-a" }.agentIds)
        assertEquals(false, next.any { it.id == AGENT_UNASSIGNED_SECTION_ID })
    }

    @Test
    fun newSectionMovesAgentAndNeverCopiesSyntheticSection() {
        val next = requireNotNull(
            createSidebarSectionForAgent(sections, "agent-2", "section-new"),
        )
        assertEquals("section-new", next.first().id)
        assertEquals(listOf("agent-2"), next.first().agentIds)
        assertEquals(emptyList<String>(), next.first { it.id == "section-b" }.agentIds)
        assertEquals(false, next.any { it.id == AGENT_UNASSIGNED_SECTION_ID })
    }

    @Test
    fun noOpAndUnknownTargetsDoNotCreateMutations() {
        assertNull(moveAgentToSidebarSection(sections, "agent-1", "section-a"))
        assertNull(moveAgentToSidebarSection(sections, "agent-1", "missing"))
        assertNull(createSidebarSectionForAgent(sections, "agent-1", "section-a"))
    }
}

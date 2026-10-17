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
    @Test
    fun sectionRenameDeleteAndMovePreserveSyntheticOwnershipRules() {
        val renamed = requireNotNull(renameAgentSidebarSection(sections, "section-a", "  Team   Alpha  "))
        assertEquals("Team Alpha", renamed.first { it.id == "section-a" }.name)
        assertEquals(false, renamed.any { it.id == AGENT_UNASSIGNED_SECTION_ID })
        assertNull(renameAgentSidebarSection(sections, AGENT_UNASSIGNED_SECTION_ID, "Nope"))

        val deleted = requireNotNull(deleteAgentSidebarSection(sections, "section-a"))
        assertEquals(listOf("section-b"), deleted.map { it.id })
        assertEquals(
            "Its agents move to Unassigned. No agents are deleted.",
            agentSidebarSectionDeleteCopy("A").description,
        )

        val moved = requireNotNull(
            moveAgentSidebarSection(
                sections,
                sectionId = "section-b",
                targetId = "section-a",
                position = AgentSidebarSectionMovePosition.BEFORE,
            ),
        )
        assertEquals(listOf("section-b", "section-a"), moved.map { it.id })
        assertNull(
            moveAgentSidebarSection(
                sections,
                sectionId = AGENT_UNASSIGNED_SECTION_ID,
                targetId = "section-a",
                position = AgentSidebarSectionMovePosition.AFTER,
            ),
        )
    }

    @Test
    fun collapsedFlagIsPreservedAcrossSectionMutations() {
        val collapsed = sections.map { section ->
            if (section.id == "section-a") section.copy(isCollapsed = true) else section
        }
        val moved = requireNotNull(moveAgentToSidebarSection(collapsed, "agent-2", "section-a"))
        assertEquals(true, moved.first { it.id == "section-a" }.isCollapsed)
        val renamed = requireNotNull(renameAgentSidebarSection(collapsed, "section-a", "Renamed"))
        assertEquals(true, renamed.first { it.id == "section-a" }.isCollapsed)
    }

    @Test
    fun sectionActionsRejectBlankNoopAndUnknownTargets() {
        assertNull(renameAgentSidebarSection(sections, "section-a", "   "))
        assertNull(renameAgentSidebarSection(sections, "missing", "Name"))
        assertNull(deleteAgentSidebarSection(sections, "missing"))
        assertNull(
            moveAgentSidebarSection(
                sections,
                sectionId = "section-a",
                targetId = "missing",
                position = AgentSidebarSectionMovePosition.BEFORE,
            ),
        )
        assertNull(
            moveAgentSidebarSection(
                sections,
                sectionId = "section-a",
                targetId = "section-a",
                position = AgentSidebarSectionMovePosition.AFTER,
            ),
        )
    }

    @Test
    fun bulkMoveUsesOneCanonicalMutationAndNeverPersistsSyntheticSection() {
        val next = requireNotNull(moveAgentsToSidebarSection(sections, setOf("agent-1", "agent-3"), "section-b"))
        assertEquals(emptyList<String>(), next.first { it.id == "section-a" }.agentIds)
        assertEquals(listOf("agent-2", "agent-1", "agent-3"), next.first { it.id == "section-b" }.agentIds)
        assertEquals(false, next.any { it.id == AGENT_UNASSIGNED_SECTION_ID })
    }

    @Test
    fun bulkMoveRejectsNoopEmptyAndUnknownTarget() {
        assertNull(moveAgentsToSidebarSection(sections, emptySet(), "section-a"))
        assertNull(moveAgentsToSidebarSection(sections, setOf("agent-1"), "section-a"))
        assertNull(moveAgentsToSidebarSection(sections, setOf("agent-1"), "missing"))
    }

    @Test
    fun adaptivePaddingCoversPhoneTabletAndFoldableWidths() {
        assertEquals(0, agentSidebarContentPaddingDp(412))
        assertEquals(32, agentSidebarContentPaddingDp(600))
        assertEquals(72, agentSidebarContentPaddingDp(840))
    }

    @Test
    fun sectionSemanticsExposeStateRoleAndAction() {
        assertEquals(
            "Collapsed section Team with 3 Agents. Toggle section",
            agentSidebarSectionSemanticsLabel("Team", 3, true),
        )
        assertEquals(
            "Expanded section Section with 0 Agents. Toggle section",
            agentSidebarSectionSemanticsLabel("", 0, false),
        )
    }

}

package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentRosterPresentationParityTest {
    @Test
    fun renameCommitMatchesDesktopTrimNonemptyAndChangedRule() {
        assertEquals("Renamed", committedAgentName("Original", "  Renamed  "))
        assertNull(committedAgentName("Original", "  Original  "))
        assertNull(committedAgentName("Original", "   "))
    }

    @Test
    fun rowActionsPreserveDesktopOrderLabelsAndHiddenFence() {
        val actions = agentRowActions(
            isHidden = false,
            isPinned = false,
            hasUnread = true,
            includeCopy = true,
            includeDelete = true,
            includeDuplicate = true,
            includeMarkUnread = true,
            includePin = true,
        )
        assertEquals(
            listOf(
                AgentRowActionId.PIN_AGENT,
                AgentRowActionId.MARK_READ,
                AgentRowActionId.DUPLICATE_AGENT,
                AgentRowActionId.COPY_CONVERSATION_ID,
                AgentRowActionId.HIDE_FROM_SIDEBAR,
                AgentRowActionId.DELETE_AGENT,
            ),
            actions.map(AgentRowAction::id),
        )
        assertEquals(
            listOf("Pin", "Mark as Read", "Duplicate", "Copy conversation ID", "Hide from sidebar", "Delete"),
            actions.map(AgentRowAction::label),
        )
        assertTrue(agentRowActions(isHidden = true, includeDelete = true, includePin = true).isEmpty())
    }

    @Test
    fun rowActionHelpersDoNotConflateSideEffects() {
        val pin = AgentRowAction(AgentRowActionId.PIN_AGENT, "Pin")
        val unpin = AgentRowAction(AgentRowActionId.UNPIN_AGENT, "Unpin")
        val read = AgentRowAction(AgentRowActionId.MARK_READ, "Mark as Read")
        val unread = AgentRowAction(AgentRowActionId.MARK_UNREAD, "Mark as Unread")
        val delete = AgentRowAction(AgentRowActionId.DELETE_AGENT, "Delete")
        val duplicate = AgentRowAction(AgentRowActionId.DUPLICATE_AGENT, "Duplicate")

        assertTrue(isTogglePinAction(pin))
        assertTrue(togglePinValue(pin))
        assertTrue(isTogglePinAction(unpin))
        assertFalse(togglePinValue(unpin))
        assertTrue(isMarkAgentUnreadAction(read))
        assertFalse(markAgentUnreadValue(read))
        assertTrue(isMarkAgentUnreadAction(unread))
        assertTrue(markAgentUnreadValue(unread))
        assertTrue(isDeleteAgentAction(delete))
        assertFalse(isDuplicateAgentAction(delete))
        assertTrue(isDuplicateAgentAction(duplicate))
        assertFalse(isDeleteAgentAction(duplicate))
    }

    @Test
    fun deleteCopyDistinguishesGroupFromSingleAgent() {
        assertEquals(
            "This permanently deletes the agent and its chat history. This can't be undone.",
            agentDeleteDescription(AgentDeleteTarget(id = "agent-1", name = "One")),
        )
        assertEquals(
            "This permanently deletes the group and its chat history. The Bots in it are not deleted and remain available individually. This can't be undone.",
            agentDeleteDescription(AgentDeleteTarget(id = "group-1", name = "Group", isGroup = true)),
        )
    }
}

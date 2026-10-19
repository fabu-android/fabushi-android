package com.ombhrum.fabushi

import org.json.JSONArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class EditorSuggestionProjectionTest {
    @Test
    fun trailingMentionAndWorkflowQueriesAreBoundedAndUrlSlashDoesNotTrigger() {
        assertEquals(
            MobileEditorSuggestionQuery('@', "ali", 6),
            findMobileEditorSuggestionQuery("hello @ali"),
        )
        assertEquals(
            MobileEditorSuggestionQuery('/', "build", 0),
            findMobileEditorSuggestionQuery("/build"),
        )
        assertNull(findMobileEditorSuggestionQuery("https://example.test/path"))
        assertNull(findMobileEditorSuggestionQuery("hello @a/b"))
    }

    @Test
    fun mentionProjectionFiltersHiddenDedupesAndScopesScheduledAutomationToAgent() {
        val agents = JSONArray(
            """[
              {"id":"agent-a","name":"Alpha"},
              {"id":"agent-b","name":"Beta","isGroup":true,"memberIds":["a","b"]},
              {"id":"agent-b","name":"Duplicate"},
              {"id":"hidden","name":"Hidden","isHiddenFromSidebar":true}
            ]""",
        )
        val automations = JSONArray(
            """[
              {"id":"daily","name":"Daily Brief","schedule":"0 8 * * *","agent_id":"agent-a","trigger_description":"Every day at 8"},
              {"id":"other","name":"Other","schedule":"0 9 * * *","agent_id":"agent-b"}
            ]""",
        )
        val rows = projectMobileEditorSuggestions(agents, automations, "agent-a", '@', "")
        assertEquals(
            listOf("everyone", "Alpha", "Beta", "Daily Brief"),
            rows.map(MobileEditorSuggestion::label),
        )
        assertEquals("Every day at 8", rows.last().subtitle)
        assertTrue(rows.none { it.id == "hidden" || it.id == "other" })
    }

    @Test
    fun slashProjectionDoesNotMisclassifyScheduledAutomationAsWorkflow() {
        val rows = projectMobileEditorSuggestions(
            JSONArray("[]"),
            JSONArray(
                """[
                  {"id":"scheduled","name":"Scheduled","schedule":"0 8 * * *","agent_id":"agent-a"},
                  {"id":"workflow","name":"Workflow","schedule":"","agent_id":"agent-a"}
                ]""",
            ),
            "agent-a",
            '/',
            "",
        )
        assertEquals(listOf("Workflow"), rows.map(MobileEditorSuggestion::label))
        assertEquals(MobileEditorSuggestionCategory.WORKFLOW, rows.single().category)
    }

    @Test
    fun requestFenceRejectsAccountAgentGenerationAndDisposedCallbacks() {
        val fence = MobileEditorSuggestionRequestFence()
        val first = requireNotNull(fence.begin("slot-a", "agent-a"))
        assertTrue(fence.accepts(first, "slot-a", "agent-a"))
        val second = requireNotNull(fence.begin("slot-a", "agent-a"))
        assertFalse(fence.accepts(first, "slot-a", "agent-a"))
        assertFalse(fence.accepts(second, "slot-b", "agent-a"))
        assertFalse(fence.accepts(second, "slot-a", "agent-b"))
        assertTrue(fence.accepts(second, "slot-a", "agent-a"))
        fence.dispose()
        assertFalse(fence.accepts(second, "slot-a", "agent-a"))
        assertNull(fence.begin("slot-a", "agent-a"))
    }

    @Test
    fun applyingSuggestionReplacesOnlyActiveTrailingToken() {
        val snapshot = MobileEditorSuggestionSnapshot(
            status = MobileEditorSuggestionStatus.READY,
            rows = emptyList(),
            trigger = '@',
            query = "al",
            replaceFrom = 6,
        )
        val row = MobileEditorSuggestion(
            key = "assistants:agent-a",
            id = "agent-a",
            category = MobileEditorSuggestionCategory.ASSISTANT,
            label = "Alpha",
        )
        assertEquals("hello @Alpha ", applyMobileEditorSuggestion("hello @al", snapshot, row))
    }
}

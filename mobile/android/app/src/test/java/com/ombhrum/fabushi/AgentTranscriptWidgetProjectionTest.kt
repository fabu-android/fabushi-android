package com.ombhrum.fabushi

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentTranscriptWidgetProjectionTest {
    private fun widget(
        id: String,
        agentId: String,
        respondedValue: String? = null,
        dismissed: Boolean = false,
    ): JSONObject =
        JSONObject()
            .put("id", id)
            .put("kind", "send-message")
            .put("agentId", agentId)
            .put(
                "message",
                JSONObject()
                    .put("type", "widget")
                    .put(
                        "widget",
                        JSONObject()
                            .put("prompt", "Choose")
                            .put("helpText", "Pick one")
                            .put(
                                "options",
                                JSONArray()
                                    .put(JSONObject().put("label", "Alpha").put("value", "a"))
                                    .put(JSONObject().put("label", "Beta")),
                            )
                            .put("allowCustom", true)
                            .put("dismissOnMoveOn", true),
                    ),
            )
            .apply {
                respondedValue?.let { put("respondedValue", it) }
                if (dismissed) put("widgetDismissed", true)
            }

    @Test
    fun projectsOnlyExplicitAgentWidgetScopeAndCanonicalSettlement() {
        val entries = JSONArray()
            .put(widget("w-a", "agent-a", respondedValue = "a"))
            .put(widget("w-b", "agent-b"))
            .put(widget("w-c", "agent-a", dismissed = true))
        val projected = canonicalMobileTranscriptForAgent(entries, "agent-a")
        assertEquals(listOf("w-a", "w-c"), projected.map { it.id })
        val answered = projected.first().widget!!
        assertEquals("Choose", answered.prompt)
        assertEquals("Pick one", answered.helpText)
        assertEquals(listOf("a", "Beta"), answered.options.map { it.value })
        assertEquals("a", answered.respondedValue)
        assertTrue(answered.allowCustom)
        assertTrue(answered.dismissOnMoveOn)
        assertTrue(projected.last().widget!!.dismissed)
    }

    @Test
    fun malformedWidgetFailsClosedInsteadOfCreatingActionSurface() {
        val malformed = widget("w-a", "agent-a")
        malformed.getJSONObject("message")
            .getJSONObject("widget")
            .put("options", JSONArray())
        val projected = canonicalMobileTranscriptForAgent(JSONArray().put(malformed), "agent-a")
        assertTrue(projected.isEmpty())
    }

    @Test
    fun unresolvedWidgetRemainsActionableProjectionOnlyUntilCanonicalReplyArrives() {
        val unresolved = canonicalMobileTranscriptForAgent(
            JSONArray().put(widget("w-a", "agent-a")),
            "agent-a",
        ).single().widget!!
        assertEquals(null, unresolved.respondedValue)
        assertFalse(unresolved.dismissed)

        val settled = canonicalMobileTranscriptForAgent(
            JSONArray().put(widget("w-a", "agent-a", respondedValue = "Beta")),
            "agent-a",
        ).single().widget!!
        assertEquals("Beta", settled.respondedValue)
    }
}

package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.parseAndroidConversationOutlineItems
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ConversationOutlineProjectionTest {
    @Test
    fun typedOutlineFiltersByAgentAndProjectsDesktopKinds() {
        val rows = JSONArray()
            .put(JSONObject().put("kind", "message").put("id", "u1").put("agentId", "a").put("role", "user").put("content", "hello"))
            .put(JSONObject().put("kind", "message").put("id", "a1").put("agentId", "a").put("role", "assistant").put("content", "hi"))
            .put(JSONObject().put("kind", "thinking").put("id", "t1").put("agentId", "a").put("text", "reasoning"))
            .put(JSONObject().put("kind", "tool-call").put("id", "x1").put("agentId", "a").put("name", "WebSearchToolCall").put("status", "running").put("summary", "lookup"))
            .put(JSONObject().put("kind", "message").put("id", "other").put("agentId", "b").put("role", "user").put("content", "secret"))
        val result = parseAndroidConversationOutlineItems(rows, "a")
        assertEquals(listOf("user", "assistant-text", "thinking", "tool-call"), result.map { it.kind })
        assertEquals("pending", result.last().status)
        assertTrue(result.none { it.id == "other" })
    }

    @Test
    fun duplicateCanonicalIdentityReconcilesInsteadOfDuplicating() {
        val rows = JSONArray()
            .put(JSONObject().put("kind", "tool-call").put("id", "tool-1").put("agentId", "a").put("name", "ShellToolCall").put("status", "running"))
            .put(JSONObject().put("kind", "tool-call").put("id", "tool-1").put("agentId", "a").put("name", "ShellToolCall").put("status", "done").put("summary", "ok"))
        val result = parseAndroidConversationOutlineItems(rows, "a")
        assertEquals(1, result.size)
        assertEquals("done", result.single().status)
        assertEquals("ok", result.single().summary)
    }

    @Test(expected = IllegalStateException::class)
    fun malformedSupportedRowsFailClosed() {
        val rows = JSONArray()
            .put(JSONObject().put("kind", "tool-call").put("id", "tool-1").put("agentId", "a").put("name", "").put("status", "done"))
        parseAndroidConversationOutlineItems(rows, "a")
    }
}

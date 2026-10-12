package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.parseAndroidFindInChatEntries
import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentFindInChatCanonicalProjectionTest {
    @Test
    fun semanticProjectionMatchesDesktopSearchPayloadsAndScopesAgent() {
        val rows = JSONArray()
            .put(
                JSONObject()
                    .put("kind", "message")
                    .put("id", "m1")
                    .put("agentId", "a")
                    .put("content", "hello")
                    .put("author", "Alice")
                    .put("attachments", JSONArray().put(JSONObject().put("name", "report.pdf"))),
            )
            .put(
                JSONObject()
                    .put("kind", "notice")
                    .put("id", "n1")
                    .put("agentId", "a")
                    .put("text", "connected"),
            )
            .put(
                JSONObject()
                    .put("kind", "send-message")
                    .put("id", "w1")
                    .put("agentId", "a")
                    .put(
                        "message",
                        JSONObject()
                            .put("type", "widget")
                            .put("widget", JSONObject().put("prompt", "Choose one")),
                    ),
            )
            .put(
                JSONObject()
                    .put("kind", "send-message")
                    .put("id", "e1")
                    .put("agentId", "a")
                    .put(
                        "message",
                        JSONObject()
                            .put("type", "email-draft")
                            .put(
                                "draft",
                                JSONObject().put("subject", "Status").put("body", "Ready to ship"),
                            ),
                    ),
            )
            .put(
                JSONObject()
                    .put("kind", "send-message")
                    .put("id", "s1")
                    .put("agentId", "a")
                    .put(
                        "message",
                        JSONObject()
                            .put("type", "slack-draft")
                            .put("draft", JSONObject().put("body", "Slack body")),
                    ),
            )
            .put(
                JSONObject()
                    .put("kind", "message")
                    .put("id", "other")
                    .put("agentId", "b")
                    .put("content", "must not leak"),
            )

        val entries = parseAndroidFindInChatEntries(rows, "a")
        assertEquals(listOf("m1", "n1", "w1", "e1", "s1"), entries.map { it.entryId })
        assertEquals("hello\nAlice\nreport.pdf", entries.first().searchText)
        assertTrue(entries.none { "must not leak" in it.searchText })
        assertEquals(
            listOf(FindInChatMatch("m1", 0)),
            findInCanonicalAgentChatMatches(entries, "report.pdf"),
        )
        assertEquals(
            listOf(FindInChatMatch("e1", 0)),
            findInCanonicalAgentChatMatches(entries, "Ready to ship"),
        )
    }

    @Test
    fun occurrenceNavigationPreservesDesktopPerOccurrenceSemantics() {
        val entries = listOf(
            com.ombhrum.fabushi.androidpreload.runtime.AndroidFindInChatEntry(
                entryId = "m1",
                searchText = "one one one",
            ),
        )
        assertEquals(
            listOf(
                FindInChatMatch("m1", 0),
                FindInChatMatch("m1", 1),
                FindInChatMatch("m1", 2),
            ),
            findInCanonicalAgentChatMatches(entries, "one"),
        )
        assertEquals(0, stepAgentFindInChatIndex(-1, 1, 3))
        assertEquals(1, stepAgentFindInChatIndex(-1, -1, 3))
    }
}

package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentFindInChatProjectionTest {
    @Test
    fun searchesMessageOccurrencesAndWidgetPromptButNotActionChrome() {
        val messages = listOf(
            MobileChatMessage(
                id = "m1",
                role = MobileChatRole.USER,
                text = "Alpha alpha",
            ),
            MobileChatMessage(
                id = "w1",
                role = MobileChatRole.ASSISTANT,
                text = "ignored fallback",
                kind = MobileChatEntryKind.WIDGET,
                widget = MobileWidgetCard(
                    prompt = "Choose Alpha",
                    options = emptyList(),
                ),
            ),
            MobileChatMessage(
                id = "a1",
                role = MobileChatRole.ASSISTANT,
                text = "",
                kind = MobileChatEntryKind.ACTION,
                actionTitle = "Alpha tool",
            ),
        )

        assertEquals(
            listOf(
                FindInChatMatch("m1", 0),
                FindInChatMatch("m1", 1),
                FindInChatMatch("w1", 0),
            ),
            findInAgentChatMatches(messages, "alpha"),
        )
        assertTrue(findInAgentChatMatches(messages, "tool").isEmpty())
    }

    @Test
    fun blankQueryHasNoMatchesAndQueryWhitespaceIsOtherwiseLiteral() {
        val message = MobileChatMessage(
            id = "m1",
            role = MobileChatRole.ASSISTANT,
            text = "alpha beta",
        )
        assertTrue(findInAgentChatMatches(listOf(message), "   ").isEmpty())
        assertEquals(
            listOf(FindInChatMatch("m1", 0)),
            findInAgentChatMatches(listOf(message), "alpha "),
        )
    }

    @Test
    fun steppingMirrorsDesktopNullSelectionAndWrapSemantics() {
        assertEquals(0, stepAgentFindInChatIndex(-1, 1, 3))
        assertEquals(1, stepAgentFindInChatIndex(-1, -1, 3))
        assertEquals(0, stepAgentFindInChatIndex(2, 1, 3))
        assertEquals(2, stepAgentFindInChatIndex(0, -1, 3))
        assertEquals(-1, stepAgentFindInChatIndex(-1, 1, 0))
    }
}

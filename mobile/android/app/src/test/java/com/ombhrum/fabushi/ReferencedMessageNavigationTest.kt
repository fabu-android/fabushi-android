package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Test

class ReferencedMessageNavigationTest {
    private fun message(id: String): ChatMessage =
        ChatMessage(
            id = id,
            conversationId = "conversation",
            text = id,
            contentType = "text",
            outgoing = false,
            time = "",
        )

    @Test
    fun targetIndexUsesCanonicalTranscriptOrder() {
        val messages = listOf(message("m1"), message("m2"), message("m3"))
        assertEquals(1, referencedMessageTargetIndex(messages, "m2"))
        assertEquals(0, referencedMessageTargetIndex(messages, "m1"))
    }

    @Test
    fun missingOrNullTargetNeverInventsNavigationDestination() {
        val messages = listOf(message("m1"))
        assertEquals(-1, referencedMessageTargetIndex(messages, "missing"))
        assertEquals(-1, referencedMessageTargetIndex(messages, null))
    }
}

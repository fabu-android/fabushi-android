package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidContentSearchMessage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class CommandPaletteMessageProviderTest {
    @Test
    fun projectsHostOwnedContentSearchMatchesWithoutReSearchingTranscript() {
        val projected = commandPaletteMessagesFromHostMatches(
            listOf(
                AndroidContentSearchMessage(
                    agentId = "agent-a",
                    entryId = "entry-user",
                    role = "user",
                    timestampMs = 20L,
                    snippet = "User match",
                ),
                AndroidContentSearchMessage(
                    agentId = "agent-b",
                    entryId = "entry-assistant",
                    role = "assistant",
                    timestampMs = 10L,
                    snippet = "Assistant match",
                ),
            ),
        )

        assertEquals(listOf("entry-user", "entry-assistant"), projected.map { it.entryId })
        assertEquals(listOf(MobileChatRole.USER, MobileChatRole.ASSISTANT), projected.map { it.role })
        assertEquals(listOf("User match", "Assistant match"), projected.map { it.snippet })
    }

    @Test
    fun rejectsMalformedHostContentSearchProjection() {
        assertThrows(IllegalStateException::class.java) {
            commandPaletteMessagesFromHostMatches(
                listOf(
                    AndroidContentSearchMessage(
                        agentId = "agent-a",
                        entryId = "entry-a",
                        role = "system",
                        timestampMs = 1L,
                        snippet = "invalid role",
                    ),
                ),
            )
        }
        assertThrows(IllegalArgumentException::class.java) {
            commandPaletteMessagesFromHostMatches(
                listOf(
                    AndroidContentSearchMessage(
                        agentId = "",
                        entryId = "entry-a",
                        role = "user",
                        timestampMs = 1L,
                        snippet = "missing agent",
                    ),
                ),
            )
        }
    }
}

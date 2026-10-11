package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidClientSideToolV2Event
import com.ombhrum.fabushi.androidpreload.runtime.AndroidToolResultCard
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentToolResultCardProjectionTest {
    private fun event(
        sequence: Long,
        agentId: String = "agent-a",
        status: String = "running",
        output: String = "",
        diff: String = "",
        kind: String = "shell",
    ) = AndroidClientSideToolV2Event(
        agentId = agentId,
        epoch = "epoch-a",
        sequence = sequence,
        projection = AndroidToolResultCard(
            kind = kind,
            toolCallId = "tool-a",
            status = status,
            path = if (kind.startsWith("file-")) "src/main.rs" else null,
            command = if (kind == "shell") "printf hi" else null,
            workingDirectory = if (kind == "shell") "/work" else null,
            summary = "",
            output = output,
            diff = diff,
            isStreaming = status == "running",
            isBackground = status == "background",
        ),
    )

    @Test
    fun latestSequenceWinsAndOtherAgentIsFencedOut() {
        val projected = projectClientSideToolResultCards(
            listOf(
                event(1, output = "partial\r\n"),
                event(2, status = "success", output = "done\r", diff = "@@\r\n"),
                event(3, agentId = "agent-b", status = "error"),
            ),
            "agent-a",
        )
        assertEquals(1, projected.size)
        val card = projected.single().toolResult!!
        assertEquals("success", card.status)
        assertEquals("done\n", card.output)
        assertEquals("@@\n", card.diff)
        assertEquals("printf hi", card.command)
        assertEquals(MobileChatEntryKind.TOOL_RESULT, projected.single().kind)
    }

    @Test
    fun fileWriteAndBackgroundStatusRemainTyped() {
        val file = projectClientSideToolResultCards(
            listOf(event(1, status = "success", kind = "file-write")),
            "agent-a",
        ).single().toolResult!!
        assertEquals("file-write", file.kind)
        assertEquals("src/main.rs", file.path)

        val background = projectClientSideToolResultCards(
            listOf(event(1, status = "background")),
            "agent-a",
        ).single().toolResult!!
        assertTrue(background.isBackground)
        assertEquals("background", background.status)
    }
}

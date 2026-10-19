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
        accountSlot = "host",
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
        val canonical = listOf(
            MobileChatMessage(
                id = "transcript-tool-a",
                role = MobileChatRole.ASSISTANT,
                text = "",
                kind = MobileChatEntryKind.ACTION,
                actionTitle = "Terminal",
                actionStatus = "running",
                toolCallId = "tool-a",
            ),
        )
        val projected = joinClientSideToolResultCards(
            canonical,
            listOf(
                event(1, output = "partial\r\n"),
                event(2, status = "success", output = "done\r", diff = "@@\r\n"),
                event(3, agentId = "agent-b", status = "error"),
            ),
            "agent-a",
        )
        assertEquals(1, projected.size)
        assertEquals("transcript-tool-a", projected.single().id)
        val card = projected.single().toolResult!!
        assertEquals("success", card.status)
        assertEquals("done\n", card.output)
        assertEquals("@@\n", card.diff)
        assertEquals("printf hi", card.command)
        assertEquals(MobileChatEntryKind.TOOL_RESULT, projected.single().kind)
    }

    @Test
    fun typedUpdateNeverCreatesASecondTranscriptRowWithoutCanonicalToolCall() {
        val ordinary = MobileChatMessage("message-1", MobileChatRole.ASSISTANT, "hello")
        val projected = joinClientSideToolResultCards(
            listOf(ordinary),
            listOf(event(1, status = "success")),
            "agent-a",
        )
        assertEquals(listOf(ordinary), projected)
    }

    @Test
    fun fileWriteAndBackgroundStatusRemainTyped() {
        val canonical = listOf(
            MobileChatMessage(
                id = "tool-row",
                role = MobileChatRole.ASSISTANT,
                text = "",
                kind = MobileChatEntryKind.ACTION,
                toolCallId = "tool-a",
            ),
        )
        val file = joinClientSideToolResultCards(
            canonical,
            listOf(event(1, status = "success", kind = "file-write")),
            "agent-a",
        ).single().toolResult!!
        assertEquals("file-write", file.kind)
        assertEquals("src/main.rs", file.path)

        val background = joinClientSideToolResultCards(
            canonical,
            listOf(event(1, status = "background")),
            "agent-a",
        ).single().toolResult!!
        assertTrue(background.isBackground)
        assertEquals("background", background.status)
    }

}

package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TranscriptAdjacencyProjectionTest {
    private fun message(
        role: TranscriptAdjacencyRole,
        author: String,
        text: String = "hello",
        sourceKind: String? = null,
        attachmentCount: Int = 0,
        presentationKind: String? = null,
        hasReaction: Boolean = false,
        hasThreadChip: Boolean = false,
    ) = TranscriptAdjacencyInput(
        kind = TranscriptAdjacencyKind.MESSAGE,
        role = role,
        author = author,
        sourceKind = sourceKind,
        text = text,
        attachmentCount = attachmentCount,
        presentationKind = presentationKind,
        hasReaction = hasReaction,
        hasThreadChip = hasThreadChip,
    )

    @Test
    fun sameAuthorBubblesContinueAndThreadChipTerminatesTheSeam() {
        val projected = projectTranscriptAdjacency(
            listOf(
                message(TranscriptAdjacencyRole.ASSISTANT, "agent-a", "first"),
                message(
                    TranscriptAdjacencyRole.ASSISTANT,
                    "agent-a",
                    "second",
                    hasThreadChip = true,
                ),
            ),
        )

        assertTrue(projected[0].isRunStart)
        assertTrue(projected[0].isContinuedToNext)
        assertTrue(projected[1].isContinuedFromPrev)
        assertTrue(projected[1].isFollowedByThreadChip)
        assertFalse(projected[1].isContinuedToNext)
    }

    @Test
    fun reactionsDisableAssistantIndicatorSeaming() {
        val row = projectTranscriptAdjacency(
            listOf(
                message(
                    TranscriptAdjacencyRole.ASSISTANT,
                    "agent-a",
                    "answer",
                    hasReaction = true,
                ),
            ),
        ).single()

        assertFalse(row.isContinuedToNext)
        assertFalse(row.isFollowedByThreadChip)
    }

    @Test
    fun standaloneEmojiImageOnlyMarkdownUrlCardAndAttachmentsAreNotBubbles() {
        val projected = projectTranscriptAdjacency(
            listOf(
                message(TranscriptAdjacencyRole.USER, "user-a", "👍"),
                message(TranscriptAdjacencyRole.USER, "user-a", "![photo](blob://photo)"),
                message(
                    TranscriptAdjacencyRole.USER,
                    "user-a",
                    "https://example.test",
                    presentationKind = "url-card",
                ),
                message(
                    TranscriptAdjacencyRole.USER,
                    "user-a",
                    "caption",
                    attachmentCount = 1,
                ),
            ),
        )

        projected.forEach {
            assertFalse(it.isContinuedFromPrev)
            assertFalse(it.isContinuedToNext)
        }
        assertTrue(isStandaloneTranscriptEmoji("👍"))
        assertTrue(isStandaloneTranscriptEmoji("👩‍💻"))
        assertTrue(isStandaloneTranscriptEmoji("1️⃣"))
        assertFalse(isStandaloneTranscriptEmoji("1"))
        assertFalse(isStandaloneTranscriptEmoji("hello"))
    }

    @Test
    fun userAttachmentForcesUserIdentityAndNeverBorrowsAssistantGroup() {
        val projected = projectTranscriptAdjacency(
            listOf(
                message(
                    TranscriptAdjacencyRole.ASSISTANT,
                    "agent-a",
                    sourceKind = "user-attachment",
                    attachmentCount = 1,
                ),
                message(TranscriptAdjacencyRole.USER, "user-a", "next"),
            ),
        )

        assertTrue(projected[0].isRunStart)
        assertTrue(projected[0].isGroupEnd)
        assertTrue(projected[1].isGroupStart)
        assertTrue(projected[1].isRunStart)
        assertFalse(projected[1].isContinuedFromPrev)
    }

    @Test
    fun sendMessageFallsBackToAssistantGroupButNeverActsAsBubble() {
        val projected = projectTranscriptAdjacency(
            listOf(
                TranscriptAdjacencyInput(
                    kind = TranscriptAdjacencyKind.SEND_MESSAGE,
                    hasReaction = false,
                ),
                message(TranscriptAdjacencyRole.ASSISTANT, "agent-a", "answer"),
            ),
        )

        assertTrue(projected[0].isRunStart)
        assertFalse(projected[0].isContinuedToNext)
        assertTrue(projected[1].isGroupStart)
        assertTrue(projected[1].isRunStart)
    }

    @Test
    fun authorIdentityControlsGroupStartRunStartAndUserGroupEnd() {
        val projected = projectTranscriptAdjacency(
            listOf(
                message(TranscriptAdjacencyRole.USER, "alice", "one"),
                message(TranscriptAdjacencyRole.USER, "bob", "two"),
            ),
            isIndicatorSeamingBubble = false,
        )

        assertTrue(projected[0].isRunStart)
        assertTrue(projected[0].isGroupEnd)
        assertTrue(projected[1].isGroupStart)
        assertTrue(projected[1].isRunStart)
        assertTrue(projected[1].isGroupEnd)
    }

    @Test
    fun chatMessageAdapterPreservesSenderIdentityAndThreadChipInput() {
        val rows = listOf(
            ChatMessage(
                id = "m1",
                conversationId = "c1",
                text = "one",
                outgoing = false,
                time = "",
                senderId = "actor-a",
            ),
            ChatMessage(
                id = "m2",
                conversationId = "c1",
                text = "two",
                outgoing = false,
                time = "",
                senderId = "actor-b",
            ),
        )
        val projected = projectChatMessageTranscriptAdjacency(
            rows,
            threadChipMessageIds = setOf("m2"),
        )

        assertTrue(projected[1].isGroupStart)
        assertTrue(projected[1].isRunStart)
        assertTrue(projected[1].isFollowedByThreadChip)
        assertFalse(projected[1].isContinuedFromPrev)
    }
}

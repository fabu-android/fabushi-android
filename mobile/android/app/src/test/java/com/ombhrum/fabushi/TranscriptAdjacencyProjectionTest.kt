package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
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
        // A send-message card retains assistant grouping identity but is never itself a bubble.
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
    fun agentAdapterUsesCanonicalAuthorMetadataAndFallbackIdentity() {
        val projected = projectAgentTranscriptAdjacency(
            messages = listOf(
                MobileChatMessage(
                    id = "a1",
                    role = MobileChatRole.ASSISTANT,
                    text = "one",
                    transcriptAuthor = "Research",
                ),
                MobileChatMessage(
                    id = "a2",
                    role = MobileChatRole.ASSISTANT,
                    text = "two",
                    transcriptAuthor = "Reviewer",
                ),
                MobileChatMessage(
                    id = "u1",
                    role = MobileChatRole.USER,
                    text = "three",
                ),
            ),
            assistantAuthor = "Fallback Agent",
        )

        assertTrue(projected[0].isRunStart)
        assertTrue(projected[1].isGroupStart)
        assertTrue(projected[1].isRunStart)
        assertTrue(projected[2].isGroupStart)
        assertTrue(projected[2].isRunStart)
        assertTrue(projected[2].isGroupEnd)
    }

    @Test
    fun agentAdapterPreservesBubbleExclusionsReactionAndThreadInputs() {
        val projected = projectAgentTranscriptAdjacency(
            messages = listOf(
                MobileChatMessage(
                    id = "emoji",
                    role = MobileChatRole.USER,
                    text = "👍",
                ),
                MobileChatMessage(
                    id = "attachment",
                    role = MobileChatRole.ASSISTANT,
                    text = "caption",
                    attachments = listOf(
                        MobileTranscriptAttachment(
                            path = "fabushi://attachment",
                            name = "photo.png",
                        ),
                    ),
                ),
                MobileChatMessage(
                    id = "url",
                    role = MobileChatRole.ASSISTANT,
                    text = "https://example.test",
                    transcriptPresentationKind = "url-card",
                ),
                MobileChatMessage(
                    id = "thread",
                    role = MobileChatRole.ASSISTANT,
                    text = "threaded",
                    hasTranscriptThreadChip = true,
                ),
                MobileChatMessage(
                    id = "reaction",
                    role = MobileChatRole.ASSISTANT,
                    text = "reacted",
                    hasTranscriptReaction = true,
                ),
            ),
            assistantAuthor = "Agent",
        )

        assertFalse(projected[0].isContinuedToNext)
        assertFalse(projected[1].isContinuedToNext)
        assertFalse(projected[2].isContinuedToNext)
        assertTrue(projected[3].isFollowedByThreadChip)
        // Desktop keeps forward continuity when the following row is a same-group bubble;
        // the thread chip instead fences the following row from continuing backward.
        assertTrue(projected[3].isContinuedToNext)
        assertFalse(projected[4].isContinuedFromPrev)
        assertFalse(projected[4].isContinuedToNext)
    }

    @Test
    fun agentWidgetUsesSendMessageFallbackAndNonMessageRowsBreakContinuity() {
        val projected = projectAgentTranscriptAdjacency(
            messages = listOf(
                MobileChatMessage(
                    id = "before",
                    role = MobileChatRole.ASSISTANT,
                    text = "before",
                ),
                MobileChatMessage(
                    id = "widget",
                    role = MobileChatRole.ASSISTANT,
                    text = "choose",
                    kind = MobileChatEntryKind.WIDGET,
                ),
                MobileChatMessage(
                    id = "tool",
                    role = MobileChatRole.ASSISTANT,
                    text = "",
                    kind = MobileChatEntryKind.TOOL_RESULT,
                ),
                MobileChatMessage(
                    id = "after",
                    role = MobileChatRole.ASSISTANT,
                    text = "after",
                ),
            ),
            assistantAuthor = "Agent",
        )

        // The preceding assistant message still seams into the indicator/card surface.
        assertTrue(projected[0].isContinuedToNext)
        assertTrue(projected[1].isGroupStart)
        assertFalse(projected[1].isContinuedFromPrev)
        assertFalse(projected[1].isContinuedToNext)
        assertEquals(emptyTranscriptAdjacency(), projected[2])
        assertTrue(projected[3].isRunStart)
        assertFalse(projected[3].isContinuedFromPrev)
    }

    @Test
    fun agentUserAttachmentForcesUserSemantics() {
        val projected = projectAgentTranscriptAdjacency(
            messages = listOf(
                MobileChatMessage(
                    id = "attachment",
                    role = MobileChatRole.ASSISTANT,
                    text = "",
                    transcriptAuthor = "Agent",
                    transcriptSourceKind = "user-attachment",
                    attachments = listOf(
                        MobileTranscriptAttachment(
                            path = "fabushi://attachment",
                            name = "notes.txt",
                        ),
                    ),
                ),
            ),
            assistantAuthor = "Agent",
        ).single()

        assertTrue(projected.isRunStart)
        assertTrue(projected.isGroupEnd)
        assertFalse(projected.isContinuedFromPrev)
        assertFalse(projected.isContinuedToNext)
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

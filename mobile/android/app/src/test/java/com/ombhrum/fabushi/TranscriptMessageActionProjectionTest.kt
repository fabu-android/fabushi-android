package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TranscriptMessageActionProjectionTest {
    private fun message(
        text: String = "hello",
        contentType: String = "text",
        mediaBlobId: String? = null,
        deliveryState: String = "sent",
    ) = ChatMessage(
        id = "m1",
        conversationId = "c1",
        text = text,
        contentType = contentType,
        mediaBlobId = mediaBlobId,
        outgoing = true,
        time = "",
        deliveryState = deliveryState,
    )

    @Test
    fun sentMessageWithContentIsActionable() {
        val projection = projectTranscriptMessageActions(message())

        assertTrue(projection.actionable)
        assertTrue(projection.canReply)
        assertTrue(projection.canSelect)
    }

    @Test
    fun queuedPendingAndFailedMessagesDoNotExposeOrdinaryActions() {
        listOf("queued", "pending", "failed").forEach { delivery ->
            val projection = projectTranscriptMessageActions(message(deliveryState = delivery))

            assertFalse(projection.actionable)
            assertFalse(projection.canReply)
            assertFalse(projection.canSelect)
        }
    }

    @Test
    fun blankTextWithoutAttachmentOrRichPayloadIsNotActionable() {
        val projection = projectTranscriptMessageActions(message(text = ""))

        assertFalse(projection.actionable)
    }

    @Test
    fun attachmentOrRichPayloadRemainsActionableWithoutText() {
        assertTrue(
            projectTranscriptMessageActions(
                message(text = "", contentType = "photo", mediaBlobId = "blob-1"),
            ).actionable,
        )
        assertTrue(
            projectTranscriptMessageActions(
                message(text = "", contentType = "poll"),
            ).actionable,
        )
    }

    @Test
    fun deliveryStateComparisonIsWhitespaceAndCaseSafe() {
        assertFalse(
            projectTranscriptMessageActions(
                message(deliveryState = "  QUEUED  "),
            ).actionable,
        )
    }
}

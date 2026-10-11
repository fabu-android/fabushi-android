package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AndroidAgentAttachment
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgement
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementEntry
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementPhase
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AgentSendAcknowledgementProjectionTest {
    private fun acknowledgement(
        phase: AndroidSendAcknowledgementPhase,
        nonce: String = "nonce-a",
        generation: Long = 7L,
        reason: String? = null,
        attachments: List<AndroidAgentAttachment> = emptyList(),
        messageText: String = "hello",
    ) = AndroidSendAcknowledgement(
        accountSlot = "slot-a",
        agentId = "agent-a",
        nonce = nonce,
        priorNonces = emptyList(),
        entries = listOf(
            AndroidSendAcknowledgementEntry(
                id = nonce,
                kind = "message",
                clientNonce = nonce,
            ),
        ),
        messageText = messageText,
        phase = phase,
        generation = generation,
        failedAtMs = reason?.let { 123L },
        reason = reason,
        attachments = attachments,
    )

    @Test
    fun failedProjectionIsRetryableButOutcomeUnknownIsNot() {
        val failed = projectDurableSendAcknowledgements(
            canonical = emptyList(),
            acknowledgements = listOf(
                acknowledgement(
                    AndroidSendAcknowledgementPhase.FAILED,
                    reason = "proven not dispatched",
                ),
            ),
        ).single()
        assertEquals("failed", failed.sendStatus)
        assertEquals("nonce-a", failed.clientNonce)
        assertEquals(7L, failed.sendAcknowledgementGeneration)
        assertTrue(isExplicitSendRetryAllowed(failed))

        val unknown = projectDurableSendAcknowledgements(
            canonical = emptyList(),
            acknowledgements = listOf(
                acknowledgement(AndroidSendAcknowledgementPhase.OUTCOME_UNKNOWN),
            ),
        ).single()
        assertEquals("outcome-unknown", unknown.sendStatus)
        assertFalse(isExplicitSendRetryAllowed(unknown))
    }

    @Test
    fun canonicalEchoWinsOverOptimisticAcknowledgementProjection() {
        val canonical = MobileChatMessage(
            id = "nonce-a",
            role = MobileChatRole.USER,
            text = "hello",
            clientNonce = "nonce-a",
        )
        val projection = projectDurableSendAcknowledgements(
            canonical = listOf(canonical),
            acknowledgements = listOf(
                acknowledgement(AndroidSendAcknowledgementPhase.ACCEPTED_AWAITING_ECHO),
            ),
        )
        assertEquals(1, projection.size)
        assertEquals(canonical, projection.single())
        assertEquals(null, projection.single().sendStatus)
    }


    @Test
    fun optimisticAcknowledgementProjectsDurableAttachmentsUntilCanonicalEcho() {
        val attachment = AndroidAgentAttachment(
            id = "agent-attachment-1",
            path = "fabushi-attachment://agent-attachment-1",
            name = "notes.txt",
            sizeBytes = 5L,
            mimeType = "text/plain",
        )
        val optimistic = projectDurableSendAcknowledgements(
            canonical = emptyList(),
            acknowledgements = listOf(
                acknowledgement(
                    phase = AndroidSendAcknowledgementPhase.ACCEPTED_AWAITING_ECHO,
                    attachments = listOf(attachment),
                    messageText = "",
                ),
            ),
        ).single()
        assertEquals("", optimistic.text)
        assertEquals(listOf("notes.txt"), optimistic.attachments.map { it.name })
        assertEquals(5L, optimistic.attachments.single().sizeBytes)
        assertEquals("fabushi-attachment://agent-attachment-1", optimistic.attachments.single().path)
    }

    @Test
    fun pendingQueuedDispatchingAndAcceptedRemainImmutableProjectionStates() {
        val phases = listOf(
            AndroidSendAcknowledgementPhase.PENDING,
            AndroidSendAcknowledgementPhase.QUEUED,
            AndroidSendAcknowledgementPhase.DISPATCHING,
            AndroidSendAcknowledgementPhase.ACCEPTED_AWAITING_ECHO,
        )
        phases.forEach { phase ->
            val row = projectDurableSendAcknowledgements(
                canonical = emptyList(),
                acknowledgements = listOf(acknowledgement(phase)),
            ).single()
            assertEquals(phase.wireName, row.sendStatus)
            assertFalse(isExplicitSendRetryAllowed(row))
        }
    }

    @Test
    fun onlyQueuedProjectionAllowsCancellation() {
        val queued = projectDurableSendAcknowledgements(
            canonical = emptyList(),
            acknowledgements = listOf(
                acknowledgement(AndroidSendAcknowledgementPhase.QUEUED),
            ),
        ).single()
        assertTrue(isQueuedSendCancellationAllowed(queued))

        val failed = projectDurableSendAcknowledgements(
            canonical = emptyList(),
            acknowledgements = listOf(
                acknowledgement(AndroidSendAcknowledgementPhase.FAILED, reason = "proven failed"),
            ),
        ).single()
        assertFalse(isQueuedSendCancellationAllowed(failed))

        val unknown = projectDurableSendAcknowledgements(
            canonical = emptyList(),
            acknowledgements = listOf(
                acknowledgement(AndroidSendAcknowledgementPhase.OUTCOME_UNKNOWN),
            ),
        ).single()
        assertFalse(isQueuedSendCancellationAllowed(unknown))
    }
}

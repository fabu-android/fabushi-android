package com.ombhrum.fabushi.androidmain.coordinator

import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgement
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementDispatch
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementPhase
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidSendSubmissionTransportQueueTest {
    private fun acknowledgement(
        nonce: String,
        generation: Long = 7L,
        phase: AndroidSendAcknowledgementPhase = AndroidSendAcknowledgementPhase.QUEUED,
    ) = AndroidSendAcknowledgement(
        accountSlot = "slot-a",
        agentId = "agent-a",
        nonce = nonce,
        priorNonces = emptyList(),
        entries = emptyList(),
        messageText = "hello",
        phase = phase,
        generation = generation,
        enteredAtMs = 1_000L,
        dispatchStartedAtMs = null,
        failedAtMs = null,
        reason = null,
    )

    private fun queue(
        transportDown: () -> Boolean,
        dispatched: MutableList<String>,
        restoredOutcomes: MutableList<String>,
    ) = AndroidSendSubmissionTransportQueue(
        isTransportDown = transportDown,
        queue = { scope -> acknowledgement(scope.nonce, scope.generation) },
        dispatch = { scope ->
            dispatched += scope.nonce
            AndroidSendAcknowledgementDispatch(
                acknowledgement = acknowledgement(
                    nonce = scope.nonce,
                    generation = scope.generation,
                    phase = AndroidSendAcknowledgementPhase.ACCEPTED_AWAITING_ECHO,
                ),
                operationId = "operation-${scope.nonce}",
            )
        },
        cancelQueuedPersisted = { true },
        onFlushed = { _, _ -> },
        onRestoredSettled = { record, outcome ->
            restoredOutcomes += "${record.nonce}:$outcome"
        },
    )

    @Test
    fun ordinaryDurableReconciliationNeverPretendsToBeProcessRestore() {
        val dispatched = mutableListOf<String>()
        val restored = mutableListOf<String>()
        val queue = queue({ false }, dispatched, restored)

        queue.discover(
            listOf(acknowledgement("ordinary")),
            trackRestoreOutcome = false,
        )
        queue.flush()

        assertEquals(listOf("ordinary"), dispatched)
        assertTrue(restored.isEmpty())
    }

    @Test
    fun reopenTrackedQueuedRecordReportsDispatchedAfterCanonicalFlush() {
        val dispatched = mutableListOf<String>()
        val restored = mutableListOf<String>()
        val queue = queue({ false }, dispatched, restored)

        queue.discover(
            listOf(acknowledgement("restored")),
            trackRestoreOutcome = true,
        )
        queue.flush()

        assertEquals(listOf("restored"), dispatched)
        assertEquals(listOf("restored:dispatched"), restored)
    }

    @Test
    fun offlineReopenReportsParkedOnceAndLaterFlushDoesNotDoubleReportRestore() {
        var offline = true
        val dispatched = mutableListOf<String>()
        val restored = mutableListOf<String>()
        val queue = queue({ offline }, dispatched, restored)

        queue.discover(
            listOf(acknowledgement("parked")),
            trackRestoreOutcome = true,
        )
        assertEquals(listOf("parked:parked"), restored)
        assertTrue(dispatched.isEmpty())

        offline = false
        queue.flush()

        assertEquals(listOf("parked"), dispatched)
        assertEquals(listOf("parked:parked"), restored)
    }
}

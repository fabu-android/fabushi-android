package com.ombhrum.fabushi.androidmain.coordinator

import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgement
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementDispatch
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSendAcknowledgementPhase
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class SendSubmissionTransportQueueTest {
    private fun acknowledgement(scope: AndroidSendSubmissionScope, phase: AndroidSendAcknowledgementPhase) =
        AndroidSendAcknowledgement(
            accountSlot = scope.accountSlot,
            agentId = scope.agentId,
            nonce = scope.nonce,
            priorNonces = emptyList(),
            entries = emptyList(),
            messageText = "hello",
            phase = phase,
            generation = scope.generation,
            failedAtMs = null,
            reason = null,
        )

    @Test
    fun transportDownQueuesAndReconnectFlushesInAgentOrder() {
        var down = true
        val queued = mutableListOf<String>()
        val dispatched = mutableListOf<String>()
        val flushed = mutableListOf<String>()
        val owner = AndroidSendSubmissionTransportQueue(
            isTransportDown = { down },
            queue = { queued += it.nonce; acknowledgement(it, AndroidSendAcknowledgementPhase.QUEUED) },
            dispatch = {
                dispatched += it.nonce
                AndroidSendAcknowledgementDispatch(
                    acknowledgement(it, AndroidSendAcknowledgementPhase.ACCEPTED_AWAITING_ECHO),
                    "op-" + it.nonce,
                )
            },
            cancelQueuedPersisted = { true },
            onFlushed = { scope, _ -> flushed += scope.nonce },
        )
        val first = AndroidSendSubmissionScope("slot-a", "agent-a", "n1", 7)
        val second = AndroidSendSubmissionScope("slot-a", "agent-a", "n2", 7)
        assertNull(owner.submit(first).operationId)
        assertNull(owner.submit(second).operationId)
        assertEquals(listOf("n1", "n2"), queued)
        assertTrue(dispatched.isEmpty())
        down = false
        owner.flush()
        assertEquals(listOf("n1", "n2"), dispatched)
        assertEquals(listOf("n1", "n2"), flushed)
    }

    @Test
    fun queuedCancellationNeverDispatchesCancelledRecord() {
        var down = true
        val cancelled = mutableListOf<String>()
        val dispatched = mutableListOf<String>()
        val owner = AndroidSendSubmissionTransportQueue(
            isTransportDown = { down },
            queue = { acknowledgement(it, AndroidSendAcknowledgementPhase.QUEUED) },
            dispatch = {
                dispatched += it.nonce
                AndroidSendAcknowledgementDispatch(
                    acknowledgement(it, AndroidSendAcknowledgementPhase.ACCEPTED_AWAITING_ECHO),
                    "op-" + it.nonce,
                )
            },
            cancelQueuedPersisted = { cancelled += it.nonce; true },
            onFlushed = { _, _ -> },
        )
        val scope = AndroidSendSubmissionScope("slot-a", "agent-a", "n1", 7)
        owner.submit(scope)
        assertTrue(owner.cancelQueued(scope))
        down = false
        owner.flush()
        assertEquals(listOf("n1"), cancelled)
        assertTrue(dispatched.isEmpty())
    }

    @Test
    fun discoveryRestoresDurableQueuedRowsAfterProcessRecreation() {
        val dispatched = mutableListOf<String>()
        val owner = AndroidSendSubmissionTransportQueue(
            isTransportDown = { false },
            queue = { acknowledgement(it, AndroidSendAcknowledgementPhase.QUEUED) },
            dispatch = {
                dispatched += it.nonce
                AndroidSendAcknowledgementDispatch(
                    acknowledgement(it, AndroidSendAcknowledgementPhase.ACCEPTED_AWAITING_ECHO),
                    "op-" + it.nonce,
                )
            },
            cancelQueuedPersisted = { true },
            onFlushed = { _, _ -> },
        )
        val scope = AndroidSendSubmissionScope("slot-a", "agent-a", "restored", 9)
        owner.discover(listOf(acknowledgement(scope, AndroidSendAcknowledgementPhase.QUEUED)))
        owner.flush()
        assertEquals(listOf("restored"), dispatched)
    }

    @Test
    fun disposeFencesLateFlushCallback() {
        var callback = false
        lateinit var owner: AndroidSendSubmissionTransportQueue
        owner = AndroidSendSubmissionTransportQueue(
            isTransportDown = { false },
            queue = { acknowledgement(it, AndroidSendAcknowledgementPhase.QUEUED) },
            dispatch = {
                owner.dispose()
                AndroidSendAcknowledgementDispatch(
                    acknowledgement(it, AndroidSendAcknowledgementPhase.ACCEPTED_AWAITING_ECHO),
                    "op-" + it.nonce,
                )
            },
            cancelQueuedPersisted = { true },
            onFlushed = { _, _ -> callback = true },
        )
        val restored = AndroidSendSubmissionScope("slot-a", "agent-a", "late", 5)
        owner.discover(listOf(acknowledgement(restored, AndroidSendAcknowledgementPhase.QUEUED)))
        owner.flush()
        assertFalse(callback)
        assertTrue(
            runCatching {
                owner.submit(AndroidSendSubmissionScope("slot-a", "agent-a", "new", 5))
            }.isFailure,
        )
    }
}

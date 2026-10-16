package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HiddenChatsMutationControllerTest {
    private data class Harness(
        val agents: MutableMap<String, HiddenChatsMutationAgent>,
        val optimistic: MutableList<Pair<String, Boolean>> = mutableListOf(),
        val rollbacks: MutableList<Triple<String, Boolean, Boolean>> = mutableListOf(),
        val pending: MutableMap<String, Boolean> = mutableMapOf(),
    ) {
        val controller = HiddenChatsMutationController(
            readAgent = agents::get,
            onOptimisticChange = { id, value ->
                optimistic += id to value
                agents[id] = agents.getValue(id).copy(isHidden = value)
            },
            onRollback = { id, optimisticValue, previousValue ->
                rollbacks += Triple(id, optimisticValue, previousValue)
                agents[id] = agents.getValue(id).copy(isHidden = previousValue)
            },
            onPendingChange = { id, value -> pending[id] = value },
        ).also { it.setScope("acct", null) }
    }

    @Test
    fun confirmedMutationStaysProjectedUntilCanonicalRosterMatches() {
        val h = Harness(mutableMapOf("a" to HiddenChatsMutationAgent("a", false)))
        val token = h.controller.begin("a", true)!!

        h.controller.settle(token, HiddenChatsMutationOutcome.CONFIRMED)
        assertFalse(h.controller.isPending("a"))
        assertTrue(h.controller.projectHidden("a", false))

        h.controller.ingestAgents(listOf(HiddenChatsMutationAgent("a", true)))
        assertTrue(h.controller.projectHidden("a", true))
    }

    @Test
    fun deterministicRejectionRollsBackOptimisticValue() {
        val h = Harness(mutableMapOf("a" to HiddenChatsMutationAgent("a", false)))
        val token = h.controller.begin("a", true)!!

        h.controller.settle(token, HiddenChatsMutationOutcome.DETERMINISTIC_REJECTION)

        assertEquals(listOf(Triple("a", true, false)), h.rollbacks)
        assertFalse(h.controller.isPending("a"))
        assertFalse(h.agents.getValue("a").isHidden)
    }

    @Test
    fun transportUnknownHoldsOptimisticValueUntilCanonicalReconciliation() {
        val h = Harness(mutableMapOf("a" to HiddenChatsMutationAgent("a", false)))
        val token = h.controller.begin("a", true)!!

        h.controller.settle(token, HiddenChatsMutationOutcome.TRANSPORT_UNKNOWN)

        assertTrue(h.controller.isPending("a"))
        assertTrue(h.controller.projectHidden("a", false))
        assertNull(h.controller.begin("a", false))

        h.controller.ingestAgents(listOf(HiddenChatsMutationAgent("a", true)))
        assertFalse(h.controller.isPending("a"))
    }

    @Test
    fun scopeChangeFencesLateCallbackAndClearsPendingProjection() {
        val h = Harness(mutableMapOf("a" to HiddenChatsMutationAgent("a", false)))
        val token = h.controller.begin("a", true)!!

        h.controller.setScope("other", null)
        h.controller.settle(token, HiddenChatsMutationOutcome.DETERMINISTIC_REJECTION)

        assertTrue(h.rollbacks.isEmpty())
        assertFalse(h.controller.isPending("a"))
        assertFalse(h.controller.projectHidden("a", false))
    }

    @Test
    fun disposeFencesLateCompletion() {
        val h = Harness(mutableMapOf("a" to HiddenChatsMutationAgent("a", false)))
        val token = h.controller.begin("a", true)!!

        h.controller.dispose()
        h.controller.settle(token, HiddenChatsMutationOutcome.CONFIRMED)

        assertFalse(h.controller.isPending("a"))
        assertFalse(h.controller.projectHidden("a", false))
    }

    @Test
    fun failureClassifierSeparatesDeterministicRejectionFromUnknownTransport() {
        assertEquals(
            HiddenChatsMutationOutcome.DETERMINISTIC_REJECTION,
            hiddenChatsMutationFailureOutcome(IllegalStateException("Agent roster mutation was rejected")),
        )
        assertEquals(
            HiddenChatsMutationOutcome.TRANSPORT_UNKNOWN,
            hiddenChatsMutationFailureOutcome(IllegalStateException("source/transport-failure")),
        )
        assertEquals(
            HiddenChatsMutationOutcome.TRANSPORT_UNKNOWN,
            hiddenChatsMutationFailureOutcome(IllegalStateException("Agent roster mutation settlement is unknown")),
        )
    }
}

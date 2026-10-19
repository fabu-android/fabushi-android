package com.ombhrum.fabushi

import org.json.JSONObject
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RosterReplicaEventFenceTest {
    private fun event(
        epoch: String,
        sequence: Long,
        replicaKey: String = "roster",
    ): JSONObject = JSONObject()
        .put(
            "ordered",
            JSONObject()
                .put("replicaKey", replicaKey)
                .put("epoch", epoch)
                .put("sequence", sequence),
        )

    @Test
    fun acceptsStrictlyIncreasingSequenceWithinEpoch() {
        val fence = RosterReplicaEventFence()
        assertTrue(fence.accept(event("epoch-a", 1)))
        assertTrue(fence.accept(event("epoch-a", 2)))
        assertFalse(fence.accept(event("epoch-a", 2)))
        assertFalse(fence.accept(event("epoch-a", 1)))
        assertTrue(fence.accept(event("epoch-a", 3)))
    }

    @Test
    fun newEpochMustStartAtOneAndOldHighSequenceCannotTakeOver() {
        val fence = RosterReplicaEventFence()
        assertTrue(fence.accept(event("epoch-a", 4)))
        assertFalse(fence.accept(event("epoch-old", 99)))
        assertTrue(fence.accept(event("epoch-b", 1)))
        assertFalse(fence.accept(event("epoch-a", 5)))
        assertTrue(fence.accept(event("epoch-b", 2)))
    }

    @Test
    fun rejectsMissingMalformedAndWrongReplicaOrdering() {
        val fence = RosterReplicaEventFence()
        assertFalse(fence.accept(JSONObject()))
        assertFalse(fence.accept(event("", 1)))
        assertFalse(fence.accept(event("epoch-a", 0)))
        assertFalse(fence.accept(event("epoch-a", 1, "timeline")))
        assertTrue(fence.accept(event("epoch-a", 1)))
    }

    @Test
    fun resetAllowsFreshProcessEpoch() {
        val fence = RosterReplicaEventFence()
        assertTrue(fence.accept(event("epoch-a", 3)))
        fence.reset()
        assertTrue(fence.accept(event("epoch-b", 7)))
    }
}

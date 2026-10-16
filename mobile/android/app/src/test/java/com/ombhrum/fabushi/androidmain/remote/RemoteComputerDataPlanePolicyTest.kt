package com.ombhrum.fabushi.androidmain.remote

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteComputerDataPlanePolicyTest {
    private fun fence(
        generation: Long = 9L,
        revision: Long = 4L,
        takeover: Boolean = true,
        lifecycle: String = "ready",
        sessionId: String = "remote-session-a",
    ) = RemoteComputerDataPlaneFence(
        deviceId = "device-a",
        sessionId = sessionId,
        processGeneration = generation,
        viewportRevision = revision,
        humanTakeover = takeover,
        lifecycle = lifecycle,
    )

    @Test
    fun inputRequiresExactProcessViewportAndHumanTakeoverLease() {
        val active = fence()
        assertTrue(RemoteComputerDataPlanePolicy.canSendInput(active, fence()))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(generation = 10L)))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(revision = 5L)))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(takeover = false)))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(lifecycle = "closing")))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(lifecycle = "outcome_unknown")))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(sessionId = "remote-session-b")))
    }

    @Test
    fun pointerEnvelopeCarriesProcessAndViewportFence() {
        val value = RemoteComputerDataPlanePolicy.pointerEnvelope(
            fence(),
            action = "move",
            normalizedX = 0.25f,
            normalizedY = 0.75f,
            buttonState = 1,
        )
        assertEquals("pointer", value.getString("kind"))
        assertEquals(9L, value.getLong("processGeneration"))
        assertEquals(4L, value.getLong("viewportRevision"))
        assertEquals("move", value.getString("action"))
        assertEquals(0.25, value.getDouble("x"), 0.0001)
        assertEquals(0.75, value.getDouble("y"), 0.0001)
        assertTrue(value.getString("eventId").isNotBlank())
    }

    @Test(expected = IllegalArgumentException::class)
    fun staleOrInvalidCoordinatesCannotBeSerialized() {
        RemoteComputerDataPlanePolicy.pointerEnvelope(
            fence(),
            action = "down",
            normalizedX = 1.1f,
            normalizedY = 0.5f,
            buttonState = 1,
        )
    }

    @Test
    fun keyEnvelopeCarriesSameDurableFence() {
        val value = RemoteComputerDataPlanePolicy.keyEnvelope(
            fence(),
            action = "down",
            keyCode = 66,
            metaState = 0,
            repeatCount = 0,
        )
        assertEquals("key", value.getString("kind"))
        assertEquals(9L, value.getLong("processGeneration"))
        assertEquals(4L, value.getLong("viewportRevision"))
        assertEquals(66, value.getInt("keyCode"))
    }
}

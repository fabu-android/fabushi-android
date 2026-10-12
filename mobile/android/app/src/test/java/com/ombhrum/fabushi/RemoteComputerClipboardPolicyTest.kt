package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.remote.RemoteComputerDataPlaneFence
import com.ombhrum.fabushi.androidmain.remote.RemoteComputerDataPlanePolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class RemoteComputerClipboardPolicyTest {
    private fun fence(
        humanTakeover: Boolean = true,
        viewportRevision: Long = 7L,
        lifecycle: String = "connected",
    ) = RemoteComputerDataPlaneFence(
        deviceId = "device-a",
        sessionId = "session-a",
        processGeneration = 3L,
        viewportRevision = viewportRevision,
        humanTakeover = humanTakeover,
        lifecycle = lifecycle,
    )

    @Test
    fun clipboardEnvelopeCarriesCurrentFenceAndBoundedText() {
        val envelope = RemoteComputerDataPlanePolicy.clipboardEnvelope(fence(), "hello")
        assertEquals(1, envelope.getInt("v"))
        assertEquals("clipboard", envelope.getString("kind"))
        assertEquals(3L, envelope.getLong("processGeneration"))
        assertEquals(7L, envelope.getLong("viewportRevision"))
        assertEquals("hello", envelope.getString("text"))
        assertTrue(envelope.getString("eventId").isNotBlank())

        assertThrows(IllegalArgumentException::class.java) {
            RemoteComputerDataPlanePolicy.clipboardEnvelope(fence(), "")
        }
        assertThrows(IllegalArgumentException::class.java) {
            RemoteComputerDataPlanePolicy.clipboardEnvelope(
                fence(),
                "x".repeat(RemoteComputerDataPlanePolicy.MAX_CLIPBOARD_BYTES + 1),
            )
        }
    }

    @Test
    fun clipboardUsesSameHumanTakeoverAndViewportFenceAsOtherInput() {
        val active = fence()
        assertTrue(RemoteComputerDataPlanePolicy.canSendInput(active, active))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(humanTakeover = false)))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(viewportRevision = 8L)))
        assertFalse(RemoteComputerDataPlanePolicy.canSendInput(active, fence(lifecycle = "outcome_unknown")))
    }
}

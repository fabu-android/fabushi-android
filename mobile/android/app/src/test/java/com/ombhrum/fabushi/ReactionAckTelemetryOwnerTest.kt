package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.SocketTimeoutException

class ReactionAckTelemetryOwnerTest {
    @Test
    fun timeoutClassifierAcceptsOnlyExplicitTimeoutSignals() {
        assertTrue(isExplicitReactionAckTimeout(SocketTimeoutException("socket timed out")))
        assertTrue(isExplicitReactionAckTimeout(IllegalStateException("request timeout")))
        assertTrue(
            isExplicitReactionAckTimeout(
                IllegalStateException("outer", IllegalArgumentException("operation timed out")),
            ),
        )

        assertFalse(isExplicitReactionAckTimeout(IllegalStateException("permission denied")))
        assertFalse(isExplicitReactionAckTimeout(IllegalArgumentException("host rejected request")))
    }
}

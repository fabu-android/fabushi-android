package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.AccessBlockedTelemetryOwner
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessBlockReason
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AccessBlockedTelemetryOwnerTest {
    @Test
    fun edgesAreAccountFencedAndDurationNeverGoesNegative() {
        val telemetry = AccessBlockedTelemetryOwner()
        val blocked = AccountAccessProjection.initial(4L).copy(
            loggedIn = true,
            sandAccessState = AccountAccessState.PAYMENT_REQUIRED,
            blockReason = AccountAccessBlockReason.PAYWALL_INDIVIDUAL,
        )

        val entered = telemetry.observe(blocked, 1_000L)
        assertEquals("entered", entered?.edge)
        assertEquals("unknown", entered?.cause)
        assertEquals("paywallIndividual", entered?.accessReason)
        assertNull(entered?.msBlocked)
        assertNull(telemetry.observe(blocked, 1_500L))

        val recovered = telemetry.observe(
            blocked.copy(
                sandAccessState = AccountAccessState.GRANTED,
                blockReason = AccountAccessBlockReason.NONE,
            ),
            2_750L,
        )
        assertEquals("recovered", recovered?.edge)
        assertEquals("paywallIndividual", recovered?.accessReason)
        assertEquals(1_750L, recovered?.msBlocked)

        val switched = blocked.copy(accountEpoch = 5L)
        val newAccount = telemetry.observe(switched, 3_000L)
        assertEquals("entered", newAccount?.edge)
        assertNull(newAccount?.msBlocked)

        val loggedOut = telemetry.observe(
            switched.copy(
                loggedIn = false,
                sandAccessState = AccountAccessState.UNKNOWN,
            ),
            2_000L,
        )
        assertEquals(0L, loggedOut?.msBlocked)
    }
}

package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionPhase
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionResilienceProjection
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionTransportState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class RootShellStatusBadgeTest {
    private fun connection(
        transport: RootConnectionTransportState,
        phase: RootConnectionPhase = RootConnectionPhase.CONNECTED,
    ) = RootConnectionResilienceProjection.initial().copy(
        accountGeneration = 1L,
        accountEpoch = 1L,
        accountIdentity = "account",
        transport = transport,
        phase = phase,
    )

    @Test fun hiddenBeforeLoggedInOrWhileRootPhaseHidden() {
        assertNull(rootShellStatusBadgeProjection(false, connection(RootConnectionTransportState.CONNECTED)))
        assertNull(
            rootShellStatusBadgeProjection(
                true,
                connection(RootConnectionTransportState.CONNECTING, RootConnectionPhase.HIDDEN),
            ),
        )
    }

    @Test fun transportMapsToDesktopEquivalentLabelsAndTones() {
        assertEquals(
            RootShellStatusBadgeProjection(RootShellStatusBadgeTone.WORKING, "Connecting"),
            rootShellStatusBadgeProjection(true, connection(RootConnectionTransportState.CONNECTING)),
        )
        assertEquals(
            RootShellStatusBadgeProjection(RootShellStatusBadgeTone.INFO, "Connected"),
            rootShellStatusBadgeProjection(true, connection(RootConnectionTransportState.CONNECTED)),
        )
        assertEquals(
            RootShellStatusBadgeProjection(RootShellStatusBadgeTone.OFFLINE, "Disconnected"),
            rootShellStatusBadgeProjection(true, connection(RootConnectionTransportState.DOWN)),
        )
    }
}

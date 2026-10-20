package com.ombhrum.fabushi.androidmain.coordinator

import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointAvailability
import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidEntrypointAvailabilityOwnerTest {
    @Test
    fun gateOnWithCanonicalAgentsIsAvailable() {
        val owner = AndroidEntrypointAvailabilityOwner(processGeneration = 7)
        val token = owner.begin(accountEpoch = 11)

        val snapshot = owner.settle(
            token = token,
            currentAccountEpoch = 11,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = true,
            rosterObserved = true,
            canonicalAgentCount = 2,
        )

        assertEquals(
            AndroidEntrypointAvailability.Available,
            snapshot.availability(AndroidEntrypointId.AGENT_NETWORK),
        )
    }

    @Test
    fun gateOnWithEmptyRosterIsRetained() {
        val owner = AndroidEntrypointAvailabilityOwner(processGeneration = 7)
        val token = owner.begin(accountEpoch = 11)

        val snapshot = owner.settle(
            token = token,
            currentAccountEpoch = 11,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = true,
            rosterObserved = true,
            canonicalAgentCount = 0,
        )

        assertEquals(
            AndroidEntrypointAvailability.Retained("empty-roster"),
            snapshot.availability(AndroidEntrypointId.AGENT_NETWORK),
        )
        assertTrue(snapshot.availability(AndroidEntrypointId.AGENT_NETWORK).isOpenable)
    }

    @Test
    fun gateOffIsUnavailable() {
        val owner = AndroidEntrypointAvailabilityOwner(processGeneration = 7)
        val token = owner.begin(accountEpoch = 11)

        val snapshot = owner.settle(
            token = token,
            currentAccountEpoch = 11,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = false,
            rosterObserved = true,
            canonicalAgentCount = 4,
        )

        assertEquals(
            AndroidEntrypointAvailability.Unavailable("gate"),
            snapshot.availability(AndroidEntrypointId.AGENT_NETWORK),
        )
    }

    @Test
    fun gateChangeReplacesCurrentProjection() {
        val owner = AndroidEntrypointAvailabilityOwner(processGeneration = 7)
        val enabled = owner.begin(accountEpoch = 11)
        owner.settle(
            enabled,
            currentAccountEpoch = 11,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = true,
            rosterObserved = true,
            canonicalAgentCount = 1,
        )

        val disabled = owner.begin(accountEpoch = 11)
        val snapshot = owner.settle(
            disabled,
            currentAccountEpoch = 11,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = false,
            rosterObserved = true,
            canonicalAgentCount = 1,
        )

        assertEquals(
            AndroidEntrypointAvailability.Unavailable("gate"),
            snapshot.availability(AndroidEntrypointId.AGENT_NETWORK),
        )
    }

    @Test
    fun staleRefreshCannotOverwriteNewerProjection() {
        val owner = AndroidEntrypointAvailabilityOwner(processGeneration = 7)
        val stale = owner.begin(accountEpoch = 11)
        val current = owner.begin(accountEpoch = 11)
        val available = owner.settle(
            current,
            currentAccountEpoch = 11,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = true,
            rosterObserved = true,
            canonicalAgentCount = 1,
        )

        val afterStale = owner.settle(
            stale,
            currentAccountEpoch = 11,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = false,
            rosterObserved = true,
            canonicalAgentCount = 0,
        )

        assertEquals(available, afterStale)
    }

    @Test
    fun accountSwitchFencesOldRefreshAndResetsAgentNetworkFailClosed() {
        val owner = AndroidEntrypointAvailabilityOwner(processGeneration = 7)
        val old = owner.begin(accountEpoch = 11)
        owner.observeAccount(accountEpoch = 12)

        val afterOld = owner.settle(
            old,
            currentAccountEpoch = 12,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = true,
            rosterObserved = true,
            canonicalAgentCount = 3,
        )

        assertEquals(12L, afterOld.accountEpoch)
        assertEquals(
            AndroidEntrypointAvailability.Unavailable("gate"),
            afterOld.availability(AndroidEntrypointId.AGENT_NETWORK),
        )
    }

    @Test
    fun activityReobservationKeepsProcessProjectionWhileProcessRecreationStartsFailClosed() {
        val owner = AndroidEntrypointAvailabilityOwner(processGeneration = 7)
        val token = owner.begin(accountEpoch = 11)
        val settled = owner.settle(
            token,
            currentAccountEpoch = 11,
            sandAgentNetworkGateObserved = true,
            sandAgentNetworkGateEnabled = true,
            rosterObserved = true,
            canonicalAgentCount = 0,
        )
        assertEquals(settled, owner.observeAccount(11))

        val recreated = AndroidEntrypointAvailabilityOwner(processGeneration = 8)
            .observeAccount(accountEpoch = 11)
        assertEquals(8L, recreated.processGeneration)
        assertEquals(
            AndroidEntrypointAvailability.Unavailable("gate"),
            recreated.availability(AndroidEntrypointId.AGENT_NETWORK),
        )
        for (id in listOf(
            AndroidEntrypointId.COMPUTER,
            AndroidEntrypointId.HIDDEN_CHATS,
            AndroidEntrypointId.PLUGINS,
            AndroidEntrypointId.SETTINGS,
        )) {
            assertEquals(AndroidEntrypointAvailability.Available, recreated.availability(id))
        }
    }
}

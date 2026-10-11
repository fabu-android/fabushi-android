package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.assertEquals
import org.junit.Test
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection
import com.ombhrum.fabushi.androidpreload.runtime.AccountRosterLoadState
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionPhase
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionResilienceProjection
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionTransportState

class RosterAccessReadinessTest {
    @Test
    fun selectionRequiresAccountLoadedConnectedAndKnownAgent() {
        val ready = selectRosterAccessReadiness(
            RosterAccessReadinessInput(
                accountKey = "account",
                transport = RosterTransportState.CONNECTED,
                loadState = RosterLoadState.READY,
                hasLoadedAgents = true,
                agentIds = listOf("a"),
                selectedAgentId = "a",
                failure = null,
                isShowingRestoredRoster = false,
                isPrivacyBlocked = false,
            ),
        )
        assertTrue(ready.isSelectionReady)
        assertTrue(ready.hasReachedBox)

        val down = selectRosterAccessReadiness(
            RosterAccessReadinessInput(
                accountKey = "account",
                transport = RosterTransportState.DOWN,
                loadState = RosterLoadState.ERROR,
                hasLoadedAgents = true,
                agentIds = listOf("a"),
                selectedAgentId = "a",
                failure = RosterFailureSnapshot("down", "gateway"),
                isShowingRestoredRoster = true,
                isPrivacyBlocked = false,
            ),
        )
        assertFalse(down.isSelectionReady)
        assertTrue(down.hasReachedBox)
        assertTrue(down.isShowingRestoredRoster)
    }

    @Test
    fun failureProjectionRejectsBlankCodes() {
        assertNull(projectRosterFailure(""))
        assertNull(projectRosterFailure(null))
        assertTrue(projectRosterFailure("offline", "gateway") != null)
    }

    @Test
    fun canonicalProjectionAdapterUsesCoordinatorTruthWithoutPersistingIdentity() {
        val access = AccountAccessProjection.initial(7L).copy(
            loggedIn = true,
            rosterLoadState = AccountRosterLoadState.READY,
            hasReachedBox = true,
            rosterFailureCode = null,
            rosterFailureTransportKind = null,
        )
        val connection = RootConnectionResilienceProjection.initial().copy(
            accountGeneration = 3L,
            accountEpoch = 7L,
            accountIdentity = "secret-account-id",
            transport = RootConnectionTransportState.CONNECTED,
            phase = RootConnectionPhase.CONNECTED,
        )
        val ready = selectRosterAccessReadiness(
            access = access,
            connection = connection,
            agentIds = listOf("agent-a"),
            selectedAgentId = "agent-a",
        )

        assertTrue(ready.isSelectionReady)
        assertEquals("epoch:7", ready.accountKey)
    }

    @Test
    fun canonicalAdapterFailsClosedForDownTransportAndPrivacyBlock() {
        val access = AccountAccessProjection.initial(2L).copy(
            loggedIn = true,
            rosterLoadState = AccountRosterLoadState.ERROR,
            hasReachedBox = true,
            rosterFailureCode = "CLOUD_AGENT_STORAGE_DISABLED",
            rosterFailureTransportKind = "no_storage",
            isShowingRestoredRoster = true,
        )
        val connection = RootConnectionResilienceProjection.initial().copy(
            accountEpoch = 2L,
            transport = RootConnectionTransportState.DOWN,
            phase = RootConnectionPhase.HIDDEN,
            isPrivacyBlocked = true,
        )
        val projected = selectRosterAccessReadiness(
            access = access,
            connection = connection,
            agentIds = listOf("agent-a"),
            selectedAgentId = "agent-a",
        )

        assertFalse(projected.isConnected)
        assertFalse(projected.isLoaded)
        assertFalse(projected.isSelectionReady)
        assertTrue(projected.hasReachedBox)
        assertTrue(projected.isShowingRestoredRoster)
        assertTrue(projected.isPrivacyBlocked)
        assertEquals("CLOUD_AGENT_STORAGE_DISABLED", projected.rosterFailureCode)
        assertEquals("no_storage", projected.rosterFailureTransportKind)
    }
}

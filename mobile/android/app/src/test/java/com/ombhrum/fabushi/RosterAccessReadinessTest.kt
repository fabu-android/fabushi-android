package com.ombhrum.fabushi

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

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
}

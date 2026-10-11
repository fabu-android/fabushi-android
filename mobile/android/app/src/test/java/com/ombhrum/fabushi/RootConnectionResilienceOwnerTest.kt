package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.RootConnectionResilienceOwner
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionPhase
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionTransportState
import org.junit.Assert.*
import org.junit.Test

class RootConnectionResilienceOwnerTest {
    private fun access(
        epoch: Long = 1L,
        loggedIn: Boolean = true,
        reached: Boolean = false,
        failureCode: String? = null,
        failureTransport: String? = null,
    ) = AccountAccessProjection.initial(epoch).copy(
        loggedIn = loggedIn,
        hasReachedBox = reached,
        rosterFailureCode = failureCode,
        rosterFailureTransportKind = failureTransport,
    )

    private fun signedIn(owner: RootConnectionResilienceOwner, identity: String = "a", epoch: Long = 1L) {
        owner.observeAccount(true, identity, epoch)
    }

    @Test fun loggedOutAndPrivacyBlockedAreHidden() {
        val owner = RootConnectionResilienceOwner()
        assertEquals(RootConnectionPhase.HIDDEN, owner.currentProjection().phase)

        signedIn(owner)
        owner.observeTransport(RootConnectionTransportState.CONNECTED)
        owner.observeReadiness(
            access(
                failureCode = "CLOUD_AGENT_STORAGE_DISABLED",
                failureTransport = "no_storage",
            ),
        )
        assertEquals(RootConnectionPhase.HIDDEN, owner.currentProjection().phase)
        assertTrue(owner.currentProjection().isPrivacyBlocked)
    }

    @Test fun phaseProjectionMatchesDesktopContract() {
        val owner = RootConnectionResilienceOwner()
        signedIn(owner)
        assertEquals(RootConnectionPhase.LOADING, owner.currentProjection().phase)

        owner.observeTransport(RootConnectionTransportState.CONNECTED)
        owner.observeReadiness(access())
        assertEquals(RootConnectionPhase.CONNECTED, owner.currentProjection().phase)

        owner.observeReadiness(access(reached = true, failureCode = "roster-load-failed"))
        assertEquals(RootConnectionPhase.RECONNECTING, owner.currentProjection().phase)

        val fresh = RootConnectionResilienceOwner()
        signedIn(fresh)
        fresh.observeTransport(RootConnectionTransportState.CONNECTED)
        fresh.observeReadiness(access(failureCode = "roster-load-failed"))
        assertEquals(RootConnectionPhase.UNREACHABLE, fresh.currentProjection().phase)
    }

    @Test fun oneRetryFlightPerGenerationAndDuplicateRequestsDoNotForkTruth() {
        val owner = RootConnectionResilienceOwner()
        signedIn(owner)
        owner.observeTransport(RootConnectionTransportState.CONNECTED)
        owner.observeReadiness(access(failureCode = "server-error"))

        val first = owner.requestRetry()
        assertNotNull(first)
        assertNull(owner.requestRetry())
        assertTrue(owner.currentProjection().isRetrying)

        assertTrue(owner.completeRetrySuccess(first!!))
        assertFalse(owner.currentProjection().isRetrying)
        assertNull(owner.currentProjection().failureCode)
    }

    @Test fun transportDownQueuesAndConnectedReleasesExactlyOneRetry() {
        val owner = RootConnectionResilienceOwner()
        signedIn(owner)
        owner.observeTransport(RootConnectionTransportState.DOWN)

        assertNull(owner.requestRetry())
        assertNull(owner.requestRetry())
        assertTrue(owner.currentProjection().retryQueued)
        assertTrue(owner.currentProjection().isRetrying)

        val released = owner.observeTransport(RootConnectionTransportState.CONNECTED)
        assertNotNull(released)
        assertNull(owner.observeTransport(RootConnectionTransportState.CONNECTED))
        assertFalse(owner.currentProjection().retryQueued)
        assertTrue(owner.currentProjection().isRetrying)
    }

    @Test fun replacementTransportFailureRequeuesInsteadOfSettlingWrongly() {
        val owner = RootConnectionResilienceOwner()
        signedIn(owner)
        owner.observeTransport(RootConnectionTransportState.CONNECTED)
        val token = owner.requestRetry()!!
        owner.observeTransport(RootConnectionTransportState.DOWN)

        assertTrue(owner.completeRetryFailure(token, "network-reset", transportDown = true))
        val p = owner.currentProjection()
        assertEquals(RootConnectionTransportState.DOWN, p.transport)
        assertTrue(p.retryQueued)
        assertNull(p.failureCode)

        assertNotNull(owner.observeTransport(RootConnectionTransportState.CONNECTED))
    }

    @Test fun deterministicConnectedFailureProjectsRealCode() {
        val owner = RootConnectionResilienceOwner()
        signedIn(owner)
        owner.observeTransport(RootConnectionTransportState.CONNECTED)
        val token = owner.requestRetry()!!

        assertTrue(owner.completeRetryFailure(token, "E_BOX_UNAVAILABLE", transportDown = false))
        assertEquals("E_BOX_UNAVAILABLE", owner.currentProjection().failureCode)
        assertFalse(owner.currentProjection().retryQueued)
        assertFalse(owner.currentProjection().isRetrying)
        assertEquals(RootConnectionPhase.UNREACHABLE, owner.currentProjection().phase)
    }

    @Test fun accountIdentityGenerationFencesOldCompletion() {
        val owner = RootConnectionResilienceOwner()
        signedIn(owner, identity = "a", epoch = 1L)
        owner.observeTransport(RootConnectionTransportState.CONNECTED)
        val stale = owner.requestRetry()!!
        val oldGeneration = stale.accountGeneration

        owner.observeAccount(true, "b", 2L)
        assertTrue(owner.currentProjection().accountGeneration > oldGeneration)
        assertFalse(owner.completeRetrySuccess(stale))
        assertEquals("b", owner.currentProjection().accountIdentity)
        assertFalse(owner.currentProjection().isRetrying)
    }

    @Test fun logoutCancelsQueuedAndInFlightWork() {
        val owner = RootConnectionResilienceOwner()
        signedIn(owner)
        owner.observeTransport(RootConnectionTransportState.CONNECTED)
        val stale = owner.requestRetry()!!

        owner.observeAccount(false, null, 2L)
        assertEquals(RootConnectionPhase.HIDDEN, owner.currentProjection().phase)
        assertFalse(owner.currentProjection().retryQueued)
        assertFalse(owner.currentProjection().isRetrying)
        assertFalse(owner.completeRetryFailure(stale, "late", transportDown = false))
    }

    @Test fun disposeInvalidatesCompletionAndSubscriptionOwnerState() {
        val owner = RootConnectionResilienceOwner()
        signedIn(owner)
        owner.observeTransport(RootConnectionTransportState.CONNECTED)
        val stale = owner.requestRetry()!!

        owner.dispose()
        assertEquals(RootConnectionPhase.HIDDEN, owner.currentProjection().phase)
        assertFalse(owner.currentProjection().isRetrying)
        assertFalse(owner.completeRetrySuccess(stale))
        assertNull(owner.observeTransport(RootConnectionTransportState.CONNECTED))
    }

    @Test fun processRecreationDoesNotReplayRetryIntent() {
        val first = RootConnectionResilienceOwner()
        signedIn(first)
        first.observeTransport(RootConnectionTransportState.DOWN)
        first.requestRetry()
        assertTrue(first.currentProjection().retryQueued)

        val reopened = RootConnectionResilienceOwner()
        reopened.observeAccount(true, "a", 1L)
        assertFalse(reopened.currentProjection().retryQueued)
        assertFalse(reopened.currentProjection().isRetrying)
        assertEquals(RootConnectionPhase.LOADING, reopened.currentProjection().phase)
    }
}

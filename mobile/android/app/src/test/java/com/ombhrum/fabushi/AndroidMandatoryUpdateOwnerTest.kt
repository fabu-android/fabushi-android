package com.ombhrum.fabushi

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AndroidMandatoryUpdateOwnerTest {
    private class MemoryStore : AndroidMandatoryUpdateProducerStateStore {
        var value: AndroidMandatoryUpdateProducerState? = null
        override fun read(): AndroidMandatoryUpdateProducerState? = value
        override fun write(state: AndroidMandatoryUpdateProducerState) {
            value = state
        }
    }

    @Test
    fun backendRequirementSurvivesProcessDeathUntilSuccessfulReconnectClearsIt() {
        val store = MemoryStore()
        val first = AndroidMandatoryUpdateOwner(store, processGeneration = 11)
        val firstToken = first.begin(accountEpoch = 7)
        val required = first.settle(
            token = firstToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = true,
                minimumVersion = "2.4.0",
                backendSignal = AndroidBackendUpdateSignal.REQUIRED,
            ),
            currentAccountEpoch = 7,
        )
        assertTrue(required.backendRequiresUpdate)
        assertEquals("2.4.0", required.minimumVersion)

        val recreated = AndroidMandatoryUpdateOwner(store, processGeneration = 12)
        val reconnectToken = recreated.begin(accountEpoch = 7)
        val transportDown = recreated.settle(
            token = reconnectToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = false,
                minimumVersion = null,
                backendSignal = AndroidBackendUpdateSignal.UNCHANGED,
            ),
            currentAccountEpoch = 7,
        )
        assertTrue(transportDown.backendRequiresUpdate)
        assertEquals("2.4.0", transportDown.minimumVersion)

        val clearToken = recreated.begin(accountEpoch = 7)
        val cleared = recreated.settle(
            token = clearToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = true,
                minimumVersion = null,
                backendSignal = AndroidBackendUpdateSignal.CLEAR,
            ),
            currentAccountEpoch = 7,
        )
        assertFalse(cleared.backendRequiresUpdate)
        assertEquals(null, cleared.minimumVersion)

        val reassertToken = recreated.begin(accountEpoch = 7)
        val reasserted = recreated.settle(
            token = reassertToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = true,
                minimumVersion = "2.5.0",
                backendSignal = AndroidBackendUpdateSignal.REQUIRED,
            ),
            currentAccountEpoch = 7,
        )
        assertTrue(reasserted.backendRequiresUpdate)
        assertEquals("2.5.0", reasserted.minimumVersion)
    }

    @Test
    fun idleGateFailsClosedWhenRefreshIsUnobservedAndSupportsReassert() {
        val store = MemoryStore()
        val first = AndroidMandatoryUpdateOwner(store, processGeneration = 31)
        val enabledToken = first.begin(accountEpoch = 12)
        val enabled = first.settle(
            token = enabledToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = false,
                minimumVersion = null,
                backendSignal = AndroidBackendUpdateSignal.UNCHANGED,
                autoUpdateWhenIdleGateObserved = true,
                autoUpdateWhenIdleGateEnabled = true,
            ),
            currentAccountEpoch = 12,
        )
        assertTrue(enabled.autoUpdateWhenIdleGateObserved)
        assertTrue(enabled.autoUpdateWhenIdleGateEnabled)

        val recreated = AndroidMandatoryUpdateOwner(store, processGeneration = 32)
        val unavailableToken = recreated.begin(accountEpoch = 12)
        val unavailable = recreated.settle(
            token = unavailableToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = false,
                minimumVersion = null,
                backendSignal = AndroidBackendUpdateSignal.UNCHANGED,
                autoUpdateWhenIdleGateObserved = false,
                autoUpdateWhenIdleGateEnabled = false,
            ),
            currentAccountEpoch = 12,
        )
        assertFalse(unavailable.autoUpdateWhenIdleGateObserved)
        assertFalse(unavailable.autoUpdateWhenIdleGateEnabled)

        val reassertToken = recreated.begin(accountEpoch = 12)
        val reasserted = recreated.settle(
            token = reassertToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = false,
                minimumVersion = null,
                backendSignal = AndroidBackendUpdateSignal.UNCHANGED,
                autoUpdateWhenIdleGateObserved = true,
                autoUpdateWhenIdleGateEnabled = true,
            ),
            currentAccountEpoch = 12,
        )
        assertTrue(reasserted.autoUpdateWhenIdleGateObserved)
        assertTrue(reasserted.autoUpdateWhenIdleGateEnabled)
    }

    @Test
    fun accountSwitchFencesLateCallbackAndClearsOldAccountRequirement() {
        val store = MemoryStore()
        val owner = AndroidMandatoryUpdateOwner(store, processGeneration = 4)
        val oldToken = owner.begin(accountEpoch = 20)
        owner.settle(
            token = oldToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = true,
                minimumVersion = "9.0.0",
                backendSignal = AndroidBackendUpdateSignal.REQUIRED,
            ),
            currentAccountEpoch = 20,
        )

        val lateToken = owner.begin(accountEpoch = 20)
        val switched = owner.observeAccount(accountEpoch = 21)
        assertFalse(switched.backendRequiresUpdate)
        assertEquals(null, switched.minimumVersion)

        val afterLateCallback = owner.settle(
            token = lateToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = true,
                minimumVersion = "99.0.0",
                backendSignal = AndroidBackendUpdateSignal.REQUIRED,
            ),
            currentAccountEpoch = 21,
        )
        assertEquals(21, afterLateCallback.accountEpoch)
        assertFalse(afterLateCallback.backendRequiresUpdate)
        assertEquals(null, afterLateCallback.minimumVersion)
    }

    @Test
    fun newerRefreshFencesOlderCallbackWithinSameAccountEpoch() {
        val store = MemoryStore()
        val owner = AndroidMandatoryUpdateOwner(store, processGeneration = 8)
        val oldToken = owner.begin(accountEpoch = 3)
        val currentToken = owner.begin(accountEpoch = 3)

        val current = owner.settle(
            token = currentToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = true,
                minimumVersion = "3.1.0",
                backendSignal = AndroidBackendUpdateSignal.CLEAR,
            ),
            currentAccountEpoch = 3,
        )
        assertEquals("3.1.0", current.minimumVersion)

        val stale = owner.settle(
            token = oldToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = true,
                minimumVersion = "99.0.0",
                backendSignal = AndroidBackendUpdateSignal.REQUIRED,
            ),
            currentAccountEpoch = 3,
        )
        assertEquals("3.1.0", stale.minimumVersion)
        assertFalse(stale.backendRequiresUpdate)
    }

    @Test
    fun idleGateSharesSequenceAndAccountFencingWithMandatoryUpdateFacts() {
        val store = MemoryStore()
        val owner = AndroidMandatoryUpdateOwner(store, processGeneration = 9)
        val staleToken = owner.begin(accountEpoch = 4)
        val currentToken = owner.begin(accountEpoch = 4)

        val current = owner.settle(
            token = currentToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = false,
                minimumVersion = null,
                backendSignal = AndroidBackendUpdateSignal.UNCHANGED,
                autoUpdateWhenIdleGateObserved = true,
                autoUpdateWhenIdleGateEnabled = false,
            ),
            currentAccountEpoch = 4,
        )
        assertTrue(current.autoUpdateWhenIdleGateObserved)
        assertFalse(current.autoUpdateWhenIdleGateEnabled)

        val stale = owner.settle(
            token = staleToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = false,
                minimumVersion = null,
                backendSignal = AndroidBackendUpdateSignal.UNCHANGED,
                autoUpdateWhenIdleGateObserved = true,
                autoUpdateWhenIdleGateEnabled = true,
            ),
            currentAccountEpoch = 4,
        )
        assertTrue(stale.autoUpdateWhenIdleGateObserved)
        assertFalse(stale.autoUpdateWhenIdleGateEnabled)

        val switchToken = owner.begin(accountEpoch = 4)
        owner.observeAccount(accountEpoch = 5)
        val afterSwitch = owner.settle(
            token = switchToken,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = false,
                minimumVersion = null,
                backendSignal = AndroidBackendUpdateSignal.UNCHANGED,
                autoUpdateWhenIdleGateObserved = true,
                autoUpdateWhenIdleGateEnabled = true,
            ),
            currentAccountEpoch = 5,
        )
        assertFalse(afterSwitch.autoUpdateWhenIdleGateObserved)
        assertFalse(afterSwitch.autoUpdateWhenIdleGateEnabled)
    }

    @Test
    fun logoutClearsDurableMandatoryUpdateFacts() {
        val store = MemoryStore()
        val owner = AndroidMandatoryUpdateOwner(store, processGeneration = 2)
        val token = owner.begin(accountEpoch = 5)
        owner.settle(
            token = token,
            facts = AndroidMandatoryUpdateFacts(
                minimumVersionObserved = true,
                minimumVersion = "7.0.0",
                backendSignal = AndroidBackendUpdateSignal.REQUIRED,
            ),
            currentAccountEpoch = 5,
        )

        val loggedOut = owner.clearForLoggedOutAccount(accountEpoch = 6)
        assertEquals(6, loggedOut.accountEpoch)
        assertEquals(null, loggedOut.minimumVersion)
        assertFalse(loggedOut.backendRequiresUpdate)
    }
}

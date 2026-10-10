package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.*
import com.ombhrum.fabushi.androidpreload.runtime.*
import org.junit.Assert.*
import org.junit.Test

class AccountAccessProjectionOwnerTest {
    private class MemoryStore(var epoch: Long = 0L, var identity: String? = null) : AccountAccessEpochStore {
        override fun readEpoch() = epoch
        override fun readIdentity() = identity
        override fun write(epoch: Long, identity: String?) { this.epoch = epoch; this.identity = identity }
    }

    @Test fun success() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val p = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true, AccountAccessState.GRANTED, AccountAccessBlockReason.NONE, AccountTruthState.GRANTED,
            AccountPaymentState.SETTLED, AccountEntitlementState.GRANTED, null, "no-storage",
            AccountTruthState.GRANTED, true, true, true, AccountRebuildState.IDLE, AccountRecoveryState.READY))
        assertTrue(p.complete); assertTrue(p.boxReady); assertFalse(p.explicitlyBlocked)
    }

    @Test fun denial() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val p = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true, AccountAccessState.PAYMENT_REQUIRED, AccountAccessBlockReason.PAYWALL_INDIVIDUAL,
            AccountTruthState.GRANTED, AccountPaymentState.REQUIRED, AccountEntitlementState.DENIED,
            null, "unknown", AccountTruthState.GRANTED, false, false, true,
            AccountRebuildState.IDLE, AccountRecoveryState.READY))
        assertTrue(p.explicitlyBlocked); assertTrue(p.mayShowAccessNotice); assertFalse(p.complete)
    }

    @Test fun staleAccountEpochIsFenced() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val stale = owner.beginRefresh(); owner.observeAuth(true, "b")
        val currentEpoch = owner.currentProjection().accountEpoch
        val p = owner.settle(stale, AccountAccessFacts(true, sandAccessState=AccountAccessState.GRANTED))
        assertEquals(currentEpoch, p.accountEpoch); assertEquals(AccountAccessState.UNKNOWN, p.sandAccessState)
    }

    @Test fun processDeathKeepsEpochButNotAccessDecision() {
        val store = MemoryStore(); val first = AccountAccessProjectionOwner(store); first.observeAuth(true, "a")
        first.settle(first.beginRefresh(), AccountAccessFacts(true, sandAccessState=AccountAccessState.GRANTED))
        val epoch = first.currentProjection().accountEpoch
        val reopened = AccountAccessProjectionOwner(store); reopened.observeAuth(true, "a")
        assertEquals(epoch, reopened.currentProjection().accountEpoch)
        assertEquals(AccountAccessState.UNKNOWN, reopened.currentProjection().sandAccessState)
    }

    @Test fun reconnectSettlesWithoutAccountSwitch() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val a = owner.settle(owner.beginRefresh(), AccountAccessFacts(true, rebuildState=AccountRebuildState.RECONNECTING, recoveryState=AccountRecoveryState.RECONNECTING))
        val b = owner.settle(owner.beginRefresh(), AccountAccessFacts(true, remoteReady=true, remoteHasDesktop=true, rebuildState=AccountRebuildState.IDLE, recoveryState=AccountRecoveryState.READY))
        assertEquals(a.accountEpoch, b.accountEpoch); assertTrue(b.boxReady)
    }

    @Test fun outcomeUnknownFailsClosed() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val p = owner.fail(owner.beginRefresh(), "unknown")
        assertEquals(AccountRecoveryState.OUTCOME_UNKNOWN, p.recoveryState)
        assertEquals(AccountPaymentState.OUTCOME_UNKNOWN, p.paymentState); assertFalse(p.complete)
    }

    @Test fun entitlementRevokeSupersedesGrant() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        owner.settle(owner.beginRefresh(), AccountAccessFacts(true, entitlementState=AccountEntitlementState.GRANTED))
        val p = owner.settle(owner.beginRefresh(), AccountAccessFacts(true, entitlementState=AccountEntitlementState.REVOKED, entitlementReason="revoked"))
        assertEquals(AccountEntitlementState.REVOKED, p.entitlementState); assertEquals("revoked", p.entitlementReason)
    }
}

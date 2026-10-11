package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidmain.coordinator.*
import com.ombhrum.fabushi.androidpreload.runtime.*
import org.junit.Assert.*
import org.junit.Test

class AccountAccessProjectionOwnerTest {
    private class MemoryStore(
        var epoch: Long = 0L,
        var identity: String? = null,
        var hasReachedBox: Boolean = false,
    ) : AccountAccessEpochStore {
        override fun readEpoch() = epoch
        override fun readIdentity() = identity
        override fun readHasReachedBox() = hasReachedBox
        override fun write(epoch: Long, identity: String?, hasReachedBox: Boolean) {
            this.epoch = epoch
            this.identity = identity
            this.hasReachedBox = hasReachedBox
        }
    }

    @Test fun success() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val p = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true, AccountAccessState.GRANTED, AccountAccessBlockReason.NONE, AccountTruthState.GRANTED,
            AccountPaymentState.SETTLED, AccountEntitlementState.GRANTED, null, "no-storage",
            AccountTruthState.GRANTED, true, true, true, AccountRebuildState.IDLE, AccountRecoveryState.READY,
            AccountRosterLoadState.READY))
        assertTrue(p.complete); assertTrue(p.boxReady); assertTrue(p.hasReachedBox); assertFalse(p.explicitlyBlocked)
    }

    @Test fun denialRequiresCanonicalRosterFailureBeforeNotice() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val hidden = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true, AccountAccessState.PAYMENT_REQUIRED, AccountAccessBlockReason.PAYWALL_INDIVIDUAL,
            AccountTruthState.GRANTED, AccountPaymentState.REQUIRED, AccountEntitlementState.DENIED,
            null, "unknown", AccountTruthState.GRANTED, false, false, true,
            AccountRebuildState.IDLE, AccountRecoveryState.READY, AccountRosterLoadState.ERROR))
        assertTrue(hidden.explicitlyBlocked); assertFalse(hidden.mayShowAccessNotice)

        val shown = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true, AccountAccessState.PAYMENT_REQUIRED, AccountAccessBlockReason.PAYWALL_INDIVIDUAL,
            AccountTruthState.GRANTED, AccountPaymentState.REQUIRED, AccountEntitlementState.DENIED,
            null, "unknown", AccountTruthState.GRANTED, false, false, true,
            AccountRebuildState.IDLE, AccountRecoveryState.READY, AccountRosterLoadState.ERROR,
            rosterFailureCode = "sand-access-blocked"))
        assertTrue(shown.mayShowAccessNotice); assertTrue(shown.complete); assertFalse(shown.isAwaitingFirstBox)
    }

    @Test fun accessNoticeCopyMatchesDesktopActionsAndFallbacks() {
        fun projection(
            reason: AccountAccessBlockReason,
            state: AccountAccessState = AccountAccessState.PAYMENT_REQUIRED,
        ) = AccountAccessProjection.initial(1L).copy(
            loggedIn = true,
            sandAccessState = state,
            blockReason = reason,
        )

        assertEquals("See Details", accountAccessNoticeCopy(projection(AccountAccessBlockReason.TEAM_PRIVACY_MODE)).action)
        assertEquals("See Details", accountAccessNoticeCopy(projection(AccountAccessBlockReason.TEAM_SETUP_REQUIRED)).action)
        assertEquals("Request Access", accountAccessNoticeCopy(projection(AccountAccessBlockReason.TEAM_ACCESS_REQUIRED)).action)
        assertNull(accountAccessNoticeCopy(projection(AccountAccessBlockReason.NOT_OFFERED)).action)
        assertEquals("Start Trial", accountAccessNoticeCopy(projection(AccountAccessBlockReason.FREE_TRIAL_AVAILABLE)).action)
        assertEquals("Upgrade", accountAccessNoticeCopy(projection(AccountAccessBlockReason.PAYWALL_INDIVIDUAL)).action)
        assertEquals("Request Access", accountAccessNoticeCopy(projection(AccountAccessBlockReason.PAYWALL_TEAM_MEMBER)).action)
        assertEquals("Manage Seats", accountAccessNoticeCopy(projection(AccountAccessBlockReason.PAYWALL_TEAM_ADMIN)).action)
        assertEquals(
            "Check Access",
            accountAccessNoticeCopy(
                projection(AccountAccessBlockReason.UNSPECIFIED, AccountAccessState.UNAVAILABLE),
            ).action,
        )
        assertEquals(
            "Check Access",
            accountAccessNoticeCopy(
                projection(AccountAccessBlockReason.NONE, AccountAccessState.PAYMENT_REQUIRED),
            ).action,
        )
        assertEquals("https://fabushi.ombhrum.com/", ACCESS_ONBOARDING_URL)
    }

    @Test fun staleAccountEpochIsFenced() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val stale = owner.beginRefresh(); owner.observeAuth(true, "b")
        val currentEpoch = owner.currentProjection().accountEpoch
        val p = owner.settle(stale, AccountAccessFacts(true, sandAccessState=AccountAccessState.GRANTED))
        assertEquals(currentEpoch, p.accountEpoch); assertEquals(AccountAccessState.UNKNOWN, p.sandAccessState)
    }

    @Test fun processDeathKeepsEpochAndDurableFirstBoxButNotAccessDecision() {
        val store = MemoryStore(); val first = AccountAccessProjectionOwner(store); first.observeAuth(true, "a")
        first.settle(first.beginRefresh(), AccountAccessFacts(
            true,
            sandAccessState = AccountAccessState.GRANTED,
            rosterLoadState = AccountRosterLoadState.READY,
        ))
        val epoch = first.currentProjection().accountEpoch
        val reopened = AccountAccessProjectionOwner(store); reopened.observeAuth(true, "a")
        assertEquals(epoch, reopened.currentProjection().accountEpoch)
        assertTrue(reopened.currentProjection().hasReachedBox)
        assertEquals(AccountAccessState.UNKNOWN, reopened.currentProjection().sandAccessState)
    }

    @Test fun accountSwitchResetsDurableFirstBox() {
        val store = MemoryStore(); val owner = AccountAccessProjectionOwner(store); owner.observeAuth(true, "a")
        owner.settle(owner.beginRefresh(), AccountAccessFacts(true, rosterLoadState=AccountRosterLoadState.READY))
        assertTrue(owner.currentProjection().hasReachedBox)
        owner.observeAuth(true, "b")
        assertFalse(owner.currentProjection().hasReachedBox)
        assertFalse(store.hasReachedBox)
    }

    @Test fun connectivityAndRestoredRosterSuppressFalseFirstBoxWait() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val network = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true,
            rosterLoadState = AccountRosterLoadState.ERROR,
            rosterFailureTransportKind = "network",
        ))
        assertFalse(network.isAwaitingFirstBox)

        val restored = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true,
            rosterLoadState = AccountRosterLoadState.ERROR,
            isShowingRestoredRoster = true,
        ))
        assertFalse(restored.isAwaitingFirstBox)

        val genuineMissing = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true,
            rosterLoadState = AccountRosterLoadState.ERROR,
            rosterFailureCode = "server-error",
        ))
        assertTrue(genuineMissing.isAwaitingFirstBox)
    }

    @Test fun reconnectSettlesWithoutAccountSwitch() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val a = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true,
            rebuildState=AccountRebuildState.RECONNECTING,
            recoveryState=AccountRecoveryState.RECONNECTING,
            rosterLoadState=AccountRosterLoadState.ERROR,
            rosterFailureTransportKind="network"))
        val b = owner.settle(owner.beginRefresh(), AccountAccessFacts(
            true,
            remoteReady=true,
            remoteHasDesktop=true,
            rebuildState=AccountRebuildState.IDLE,
            recoveryState=AccountRecoveryState.READY,
            rosterLoadState=AccountRosterLoadState.READY))
        assertEquals(a.accountEpoch, b.accountEpoch); assertTrue(b.boxReady); assertTrue(b.hasReachedBox)
    }

    @Test fun outcomeUnknownFailsClosed() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        val p = owner.fail(owner.beginRefresh(), "unknown")
        assertEquals(AccountRecoveryState.OUTCOME_UNKNOWN, p.recoveryState)
        assertEquals(AccountRosterLoadState.ERROR, p.rosterLoadState)
        assertEquals(AccountPaymentState.OUTCOME_UNKNOWN, p.paymentState); assertFalse(p.complete)
    }

    @Test fun entitlementRevokeSupersedesGrant() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        owner.settle(owner.beginRefresh(), AccountAccessFacts(true, entitlementState=AccountEntitlementState.GRANTED))
        val p = owner.settle(owner.beginRefresh(), AccountAccessFacts(true, entitlementState=AccountEntitlementState.REVOKED, entitlementReason="revoked"))
        assertEquals(AccountEntitlementState.REVOKED, p.entitlementState); assertEquals("revoked", p.entitlementReason)
    }
}

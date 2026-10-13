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

    private class FailingStore(
        epoch: Long = 0L,
        identity: String? = null,
        hasReachedBox: Boolean = false,
    ) : AccountAccessEpochStore {
        private var durableEpoch = epoch
        private var durableIdentity = identity
        private var durableHasReachedBox = hasReachedBox
        var failWrites = false

        override fun readEpoch() = durableEpoch
        override fun readIdentity() = durableIdentity
        override fun readHasReachedBox() = durableHasReachedBox

        override fun write(epoch: Long, identity: String?, hasReachedBox: Boolean) {
            if (failWrites) error("injected persistence failure")
            durableEpoch = epoch
            durableIdentity = identity
            durableHasReachedBox = hasReachedBox
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

    @Test fun failedAccountEpochPersistenceDoesNotPublishNewAccountFence() {
        val store = FailingStore()
        val owner = AccountAccessProjectionOwner(store)
        owner.observeAuth(true, "a")
        val before = owner.currentProjection()
        store.failWrites = true

        assertThrows(IllegalStateException::class.java) {
            owner.observeAuth(true, "b")
        }

        assertEquals(before.accountEpoch, owner.currentProjection().accountEpoch)
        val reopened = AccountAccessProjectionOwner(store)
        reopened.observeAuth(true, "a")
        assertEquals(before.accountEpoch, reopened.currentProjection().accountEpoch)
    }

    @Test fun failedFirstBoxPersistenceDoesNotPublishDurableLatch() {
        val store = FailingStore()
        val owner = AccountAccessProjectionOwner(store)
        owner.observeAuth(true, "a")
        store.failWrites = true
        val token = owner.beginRefresh()

        assertThrows(IllegalStateException::class.java) {
            owner.settle(
                token,
                AccountAccessFacts(
                    loggedIn = true,
                    rosterLoadState = AccountRosterLoadState.READY,
                ),
            )
        }

        assertFalse(owner.currentProjection().hasReachedBox)
        store.failWrites = false
        val reopened = AccountAccessProjectionOwner(store)
        reopened.observeAuth(true, "a")
        assertFalse(reopened.currentProjection().hasReachedBox)
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

    @Test fun rosterFailureClassificationPreservesDesktopPrivacyAndTransportSemantics() {
        val privacyCode = classifyRosterAccessFailure(
            "Host failed: CLOUD_AGENT_STORAGE_DISABLED",
        )
        assertEquals("CLOUD_AGENT_STORAGE_DISABLED", privacyCode.code)
        assertNull(privacyCode.transportKind)

        val noStorage = classifyRosterAccessFailure("transport kind=no_storage")
        assertEquals("CLOUD_AGENT_STORAGE_DISABLED", noStorage.code)
        assertEquals("no_storage", noStorage.transportKind)

        val dns = classifyRosterAccessFailure("DNS resolution failed")
        assertEquals("roster-load-failed", dns.code)
        assertEquals("dns", dns.transportKind)

        val timeout = classifyRosterAccessFailure("network timeout")
        assertEquals("roster-load-failed", timeout.code)
        assertEquals("network", timeout.transportKind)

        val unknown = classifyRosterAccessFailure("host unavailable")
        assertEquals("roster-load-failed", unknown.code)
        assertNull(unknown.transportKind)
    }

    @Test fun privacyFailureSuppressesFalseFirstBoxWait() {
        val owner = AccountAccessProjectionOwner(MemoryStore())
        owner.observeAuth(true, "a")
        val byCode = owner.settle(
            owner.beginRefresh(),
            AccountAccessFacts(
                loggedIn = true,
                rosterLoadState = AccountRosterLoadState.ERROR,
                rosterFailureCode = "CLOUD_AGENT_STORAGE_DISABLED",
            ),
        )
        assertFalse(byCode.isAwaitingFirstBox)

        val byTransport = owner.settle(
            owner.beginRefresh(),
            AccountAccessFacts(
                loggedIn = true,
                rosterLoadState = AccountRosterLoadState.ERROR,
                rosterFailureTransportKind = "no_storage",
            ),
        )
        assertFalse(byTransport.isAwaitingFirstBox)
    }

    @Test fun accessBlockedTelemetryRecoveryKeepsEnteredReasonAndCapsDuration() {
        val owner = AccessBlockedTelemetryOwner()
        val enteredProjection = AccountAccessProjection.initial(7L).copy(
            loggedIn = true,
            sandAccessState = AccountAccessState.PAYMENT_REQUIRED,
            blockReason = AccountAccessBlockReason.PAYWALL_INDIVIDUAL,
        )
        val entered = owner.observe(enteredProjection, 1_000L)!!
        assertEquals("entered", entered.edge)
        assertEquals("paywallIndividual", entered.accessReason)
        assertNull(entered.msBlocked)

        val recoveredProjection = enteredProjection.copy(
            sandAccessState = AccountAccessState.GRANTED,
            blockReason = AccountAccessBlockReason.NONE,
        )
        val recovered = owner.observe(
            recoveredProjection,
            1_000L + 8L * 24L * 60L * 60L * 1_000L,
        )!!
        assertEquals("recovered", recovered.edge)
        assertEquals(
            "paywallIndividual",
            recovered.accessReason,
            "recovery must report the reason that caused the blocked interval",
        )
        assertEquals(7L * 24L * 60L * 60L * 1_000L, recovered.msBlocked)
    }

    @Test fun accessBlockedTelemetryAccountSwitchFencesPriorBlockedInterval() {
        val owner = AccessBlockedTelemetryOwner()
        val blocked = AccountAccessProjection.initial(2L).copy(
            loggedIn = true,
            sandAccessState = AccountAccessState.PAYMENT_REQUIRED,
            blockReason = AccountAccessBlockReason.PAYWALL_TEAM_MEMBER,
        )
        assertEquals("entered", owner.observe(blocked, 5_000L)!!.edge)

        val nextAccountReady = AccountAccessProjection.initial(3L).copy(
            loggedIn = true,
            sandAccessState = AccountAccessState.GRANTED,
            blockReason = AccountAccessBlockReason.NONE,
        )
        assertNull(
            "new account epoch must not emit recovery for the prior account",
            owner.observe(nextAccountReady, 6_000L),
        )
    }

    @Test fun entitlementRevokeSupersedesGrant() {
        val owner = AccountAccessProjectionOwner(MemoryStore()); owner.observeAuth(true, "a")
        owner.settle(owner.beginRefresh(), AccountAccessFacts(true, entitlementState=AccountEntitlementState.GRANTED))
        val p = owner.settle(owner.beginRefresh(), AccountAccessFacts(true, entitlementState=AccountEntitlementState.REVOKED, entitlementReason="revoked"))
        assertEquals(AccountEntitlementState.REVOKED, p.entitlementState); assertEquals("revoked", p.entitlementReason)
    }
}

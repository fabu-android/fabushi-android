package com.ombhrum.fabushi.androidmain.coordinator

import android.content.Context
import com.ombhrum.fabushi.androidpreload.runtime.*

internal interface AccountAccessEpochStore {
    fun readEpoch(): Long
    fun readIdentity(): String?
    fun readHasReachedBox(): Boolean
    fun write(epoch: Long, identity: String?, hasReachedBox: Boolean)
}

internal class SharedPreferencesAccountAccessEpochStore(context: Context) : AccountAccessEpochStore {
    private val preferences = context.applicationContext.getSharedPreferences("fabushi-account-access-projection", 0)
    override fun readEpoch() = preferences.getLong("account-epoch", 0L).coerceAtLeast(0L)
    override fun readIdentity() = preferences.getString("account-identity", null)?.trim()?.takeIf(String::isNotEmpty)
    override fun readHasReachedBox() = preferences.getBoolean("has-reached-box", false)
    override fun write(epoch: Long, identity: String?, hasReachedBox: Boolean) {
        preferences.edit()
            .putLong("account-epoch", epoch.coerceAtLeast(0L))
            .putBoolean("has-reached-box", hasReachedBox)
            .apply {
                if (identity == null) remove("account-identity") else putString("account-identity", identity)
            }
            .apply()
    }
}

internal data class AccountAccessRefreshToken(val generation: Long, val accountEpoch: Long)
internal data class RosterAccessFailureClassification(
    val code: String,
    val transportKind: String?,
)

internal fun classifyRosterAccessFailure(message: String): RosterAccessFailureClassification {
    val normalized = message.trim()
    val lower = normalized.lowercase()
    val privacyCode = "CLOUD_AGENT_STORAGE_DISABLED" in normalized.uppercase()
    val noStorage = "no_storage" in lower || "no storage" in lower
    return when {
        privacyCode || noStorage -> RosterAccessFailureClassification(
            code = "CLOUD_AGENT_STORAGE_DISABLED",
            transportKind = if (noStorage) "no_storage" else null,
        )
        "dns" in lower -> RosterAccessFailureClassification(
            code = "roster-load-failed",
            transportKind = "dns",
        )
        "network" in lower || "connect" in lower || "timeout" in lower ->
            RosterAccessFailureClassification(
                code = "roster-load-failed",
                transportKind = "network",
            )
        else -> RosterAccessFailureClassification(
            code = "roster-load-failed",
            transportKind = null,
        )
    }
}

internal data class AccountAccessFacts(
    val loggedIn: Boolean,
    val sandAccessState: AccountAccessState = AccountAccessState.UNKNOWN,
    val blockReason: AccountAccessBlockReason = AccountAccessBlockReason.UNSPECIFIED,
    val authorizationState: AccountTruthState = AccountTruthState.UNKNOWN,
    val paymentState: AccountPaymentState = AccountPaymentState.UNKNOWN,
    val entitlementState: AccountEntitlementState = AccountEntitlementState.UNKNOWN,
    val entitlementReason: String? = null,
    val privacyMode: String = "unknown",
    val teamPolicyState: AccountTruthState = AccountTruthState.UNKNOWN,
    val remoteReady: Boolean = false,
    val remoteHasDesktop: Boolean = false,
    val sessionSettled: Boolean = false,
    val rebuildState: AccountRebuildState = AccountRebuildState.UNKNOWN,
    val recoveryState: AccountRecoveryState = AccountRecoveryState.UNKNOWN,
    val rosterLoadState: AccountRosterLoadState = AccountRosterLoadState.UNKNOWN,
    val rosterFailureCode: String? = null,
    val rosterFailureTransportKind: String? = null,
    val isShowingRestoredRoster: Boolean = false,
    val isRosterFetching: Boolean = false,
    val detail: String? = null,
)

/** Coordinator-owned epoch/generation fence for asynchronous access facts. */
internal class AccountAccessProjectionOwner(private val epochStore: AccountAccessEpochStore) {
    private var accountEpoch = epochStore.readEpoch().coerceAtLeast(0L)
    private var accountIdentity = epochStore.readIdentity()
    private var hasReachedBox = epochStore.readHasReachedBox()
    private var generation = 0L
    private var projection = AccountAccessProjection.initial(accountEpoch).copy(hasReachedBox = hasReachedBox)

    @Synchronized fun observeAuth(loggedIn: Boolean, identity: String?) {
        val normalized = identity?.trim()?.takeIf { loggedIn && it.isNotEmpty() }
        if (normalized != accountIdentity) {
            accountEpoch = next(accountEpoch)
            accountIdentity = normalized
            hasReachedBox = false
            epochStore.write(accountEpoch, accountIdentity, hasReachedBox)
            generation = next(generation)
            projection = AccountAccessProjection.initial(accountEpoch).copy(
                loggedIn = loggedIn,
                recoveryState = if (loggedIn) AccountRecoveryState.UNKNOWN else AccountRecoveryState.READY,
            )
        } else {
            projection = projection.copy(
                loggedIn = loggedIn,
                recoveryState = if (!loggedIn) AccountRecoveryState.READY else projection.recoveryState,
                hasReachedBox = hasReachedBox,
            )
        }
    }

    @Synchronized fun beginRefresh(): AccountAccessRefreshToken {
        generation = next(generation)
        return AccountAccessRefreshToken(generation, accountEpoch)
    }

    @Synchronized fun settle(token: AccountAccessRefreshToken, facts: AccountAccessFacts): AccountAccessProjection {
        if (!current(token)) return projection
        if (facts.loggedIn && accountIdentity == null) return fail(token, "account_access_identity_missing")

        val reachedNow = facts.rosterLoadState == AccountRosterLoadState.READY
        val nextHasReachedBox = hasReachedBox || reachedNow
        if (nextHasReachedBox != hasReachedBox) {
            hasReachedBox = nextHasReachedBox
            epochStore.write(accountEpoch, accountIdentity, hasReachedBox)
        }

        val connectivityFailure =
            facts.rosterFailureTransportKind == "network" || facts.rosterFailureTransportKind == "dns"
        val privacyFailure =
            facts.rosterFailureCode == "CLOUD_AGENT_STORAGE_DISABLED" ||
                facts.rosterFailureTransportKind == "no_storage"
        val firstBoxSuppressed =
            facts.isShowingRestoredRoster ||
                facts.rosterFailureCode == "sand-access-blocked" ||
                privacyFailure ||
                connectivityFailure
        val isLoading =
            facts.rosterLoadState == AccountRosterLoadState.LOADING || facts.isRosterFetching
        val isAwaitingFirstBox =
            facts.loggedIn && !isLoading && !hasReachedBox && !firstBoxSuppressed

        projection = AccountAccessProjection(
            accountEpoch = accountEpoch,
            loggedIn = facts.loggedIn,
            sandAccessState = facts.sandAccessState,
            blockReason = facts.blockReason,
            authorizationState = facts.authorizationState,
            paymentState = facts.paymentState,
            entitlementState = facts.entitlementState,
            entitlementReason = facts.entitlementReason,
            privacyMode = facts.privacyMode,
            teamPolicyState = facts.teamPolicyState,
            remoteReady = facts.remoteReady,
            remoteHasDesktop = facts.remoteHasDesktop,
            boxReady = facts.remoteReady && facts.remoteHasDesktop,
            sessionSettled = facts.sessionSettled,
            rebuildState = facts.rebuildState,
            recoveryState = facts.recoveryState,
            rosterLoadState = facts.rosterLoadState,
            rosterFailureCode = facts.rosterFailureCode,
            rosterFailureTransportKind = facts.rosterFailureTransportKind,
            isShowingRestoredRoster = facts.isShowingRestoredRoster,
            isRosterFetching = facts.isRosterFetching,
            hasReachedBox = hasReachedBox,
            isAwaitingFirstBox = isAwaitingFirstBox,
            complete = facts.loggedIn &&
                facts.sandAccessState != AccountAccessState.UNKNOWN &&
                facts.authorizationState != AccountTruthState.UNKNOWN &&
                facts.teamPolicyState != AccountTruthState.UNKNOWN &&
                facts.rebuildState != AccountRebuildState.UNKNOWN &&
                facts.recoveryState != AccountRecoveryState.OUTCOME_UNKNOWN,
            detail = facts.detail,
        )
        return projection
    }

    @Synchronized fun fail(token: AccountAccessRefreshToken, detail: String): AccountAccessProjection {
        if (!current(token)) return projection
        projection = projection.copy(
            sandAccessState = if (projection.loggedIn) AccountAccessState.UNKNOWN else projection.sandAccessState,
            paymentState = if (projection.loggedIn) AccountPaymentState.OUTCOME_UNKNOWN else projection.paymentState,
            recoveryState = AccountRecoveryState.OUTCOME_UNKNOWN,
            rosterLoadState = if (projection.loggedIn) AccountRosterLoadState.ERROR else projection.rosterLoadState,
            hasReachedBox = hasReachedBox,
            isAwaitingFirstBox = false,
            complete = false,
            detail = detail.take(240),
        )
        return projection
    }

    @Synchronized fun currentProjection() = projection
    private fun current(token: AccountAccessRefreshToken) = token.generation == generation && token.accountEpoch == accountEpoch
    private fun next(value: Long) = if (value == Long.MAX_VALUE) Long.MAX_VALUE else (value + 1L).coerceAtLeast(1L)
}

internal data class RootConnectionRetryToken(
    val accountGeneration: Long,
    val accountEpoch: Long,
    val attempt: Long,
)

/**
 * Process-scoped owner for the Desktop root connection resilience contract.
 *
 * This owner never becomes a second account/roster source of truth. It consumes canonical
 * auth epoch + immutable AccountAccessProjection facts and owns only derived connection phase,
 * a single retry flight, and the in-memory queued user retry intent. Retry intent is deliberately
 * not persisted, so a cold process starts from auth/Coordinator/roster truth instead of replaying
 * an old side effect.
 */
internal class RootConnectionResilienceOwner {
    private var accountGeneration = 0L
    private var accountEpoch = 0L
    private var accountIdentity: String? = null
    private var loggedIn = false
    private var transport = RootConnectionTransportState.CONNECTING
    private var hasReachedBox = false
    private var privacyBlocked = false
    private var failureCode: String? = null
    private var retryQueued = false
    private var inFlight: RootConnectionRetryToken? = null
    private var retryAttempt = 0L
    private var disposed = false

    @Synchronized
    fun observeAccount(
        loggedIn: Boolean,
        identity: String?,
        accountEpoch: Long,
    ): RootConnectionResilienceProjection {
        if (disposed) return projection()
        val normalizedIdentity = identity?.trim()?.takeIf { loggedIn && it.isNotEmpty() }
        val changed =
            this.loggedIn != loggedIn ||
                this.accountIdentity != normalizedIdentity ||
                this.accountEpoch != accountEpoch
        if (changed) {
            accountGeneration = next(accountGeneration)
            this.loggedIn = loggedIn
            this.accountIdentity = normalizedIdentity
            this.accountEpoch = accountEpoch
            transport = RootConnectionTransportState.CONNECTING
            hasReachedBox = false
            privacyBlocked = false
            failureCode = null
            retryQueued = false
            inFlight = null
        }
        return projection()
    }

    @Synchronized
    fun observeReadiness(access: AccountAccessProjection): RootConnectionResilienceProjection {
        if (disposed || access.accountEpoch != accountEpoch) return projection()
        loggedIn = access.loggedIn
        hasReachedBox = access.hasReachedBox
        privacyBlocked =
            access.rosterFailureCode == "CLOUD_AGENT_STORAGE_DISABLED" ||
                access.rosterFailureTransportKind == "no_storage"
        failureCode = access.rosterFailureCode
        if (!loggedIn) {
            retryQueued = false
            inFlight = null
            failureCode = null
        }
        return projection()
    }

    /**
     * Returns a newly-owned retry token only when a dispatch should start now.
     * A duplicate request in the same generation never creates a second flight.
     */
    @Synchronized
    fun requestRetry(): RootConnectionRetryToken? {
        if (disposed || !loggedIn || privacyBlocked) return null
        if (inFlight != null) return null
        if (transport != RootConnectionTransportState.CONNECTED) {
            retryQueued = true
            return null
        }
        return beginRetry()
    }

    /**
     * A canonical transport transition may release one queued user retry.
     * No queued intent is persisted across process recreation.
     */
    @Synchronized
    fun observeTransport(state: RootConnectionTransportState): RootConnectionRetryToken? {
        if (disposed) return null
        transport = state
        if (
            state == RootConnectionTransportState.CONNECTED &&
            retryQueued &&
            inFlight == null &&
            loggedIn &&
            !privacyBlocked
        ) {
            return beginRetry()
        }
        return null
    }

    @Synchronized
    fun completeRetrySuccess(token: RootConnectionRetryToken): Boolean {
        if (!current(token)) return false
        inFlight = null
        retryQueued = false
        failureCode = null
        return true
    }

    /**
     * A replacement transport disappearing during the retry is not a terminal failure.
     * Keep the single user retry queued until a later CONNECTED transition.
     */
    @Synchronized
    fun completeRetryFailure(
        token: RootConnectionRetryToken,
        code: String?,
        transportDown: Boolean,
    ): Boolean {
        if (!current(token)) return false
        inFlight = null
        if (transportDown || transport != RootConnectionTransportState.CONNECTED) {
            transport = RootConnectionTransportState.DOWN
            failureCode = null
            retryQueued = true
        } else {
            failureCode = code?.trim()?.takeIf(String::isNotEmpty) ?: "roster-load-failed"
            retryQueued = false
        }
        return true
    }

    @Synchronized fun currentProjection(): RootConnectionResilienceProjection = projection()

    @Synchronized
    fun dispose() {
        if (disposed) return
        disposed = true
        accountGeneration = next(accountGeneration)
        loggedIn = false
        accountIdentity = null
        retryQueued = false
        inFlight = null
        failureCode = null
        privacyBlocked = false
        hasReachedBox = false
        transport = RootConnectionTransportState.CONNECTING
    }

    private fun beginRetry(): RootConnectionRetryToken {
        retryQueued = false
        retryAttempt = next(retryAttempt)
        return RootConnectionRetryToken(
            accountGeneration = accountGeneration,
            accountEpoch = accountEpoch,
            attempt = retryAttempt,
        ).also { inFlight = it }
    }

    private fun current(token: RootConnectionRetryToken): Boolean =
        !disposed &&
            token == inFlight &&
            token.accountGeneration == accountGeneration &&
            token.accountEpoch == accountEpoch

    private fun projection(): RootConnectionResilienceProjection {
        val phase = when {
            !loggedIn || privacyBlocked -> RootConnectionPhase.HIDDEN
            transport == RootConnectionTransportState.CONNECTING -> RootConnectionPhase.LOADING
            transport == RootConnectionTransportState.CONNECTED && failureCode == null ->
                RootConnectionPhase.CONNECTED
            hasReachedBox -> RootConnectionPhase.RECONNECTING
            else -> RootConnectionPhase.UNREACHABLE
        }
        return RootConnectionResilienceProjection(
            accountGeneration = accountGeneration,
            accountEpoch = accountEpoch,
            accountIdentity = accountIdentity,
            transport = transport,
            phase = phase,
            hasReachedBox = hasReachedBox,
            isPrivacyBlocked = privacyBlocked,
            failureCode = failureCode,
            isRetrying = retryQueued || inFlight != null,
            retryQueued = retryQueued,
        )
    }

    private fun next(value: Long) =
        if (value == Long.MAX_VALUE) Long.MAX_VALUE else (value + 1L).coerceAtLeast(1L)
}


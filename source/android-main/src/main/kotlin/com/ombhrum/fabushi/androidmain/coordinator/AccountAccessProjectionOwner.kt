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
        val firstBoxSuppressed =
            facts.isShowingRestoredRoster ||
                facts.rosterFailureCode == "sand-access-blocked" ||
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

package com.ombhrum.fabushi.androidpreload.runtime

enum class AccountAccessState { CHECKING, GRANTED, UNAVAILABLE, PAYMENT_REQUIRED, UNKNOWN }

enum class AccountAccessBlockReason {
    NONE, TEAM_PRIVACY_MODE, TEAM_SETUP_REQUIRED, TEAM_ACCESS_REQUIRED, NOT_OFFERED,
    FREE_TRIAL_AVAILABLE, PAYWALL_INDIVIDUAL, PAYWALL_TEAM_MEMBER, PAYWALL_TEAM_ADMIN, UNSPECIFIED,
}
enum class AccountTruthState { GRANTED, DENIED, UNKNOWN }
enum class AccountPaymentState { SETTLED, REQUIRED, PENDING, OUTCOME_UNKNOWN, UNKNOWN }
enum class AccountEntitlementState { GRANTED, DENIED, REVOKED, UNKNOWN }
enum class AccountRecoveryState { READY, RECONNECTING, OUTCOME_UNKNOWN, UNKNOWN }
enum class AccountRebuildState { IDLE, RECONNECTING, OUTCOME_UNKNOWN, UNKNOWN }
enum class AccountRosterLoadState { LOADING, READY, ERROR, UNKNOWN }

enum class RootConnectionTransportState { CONNECTING, CONNECTED, DOWN }
enum class RootConnectionPhase { HIDDEN, LOADING, CONNECTED, RECONNECTING, UNREACHABLE }

/**
 * Immutable root-connection projection. Account/auth/roster truth remains Coordinator-owned;
 * presentation can only render this derived state and request a fenced retry.
 */
data class RootConnectionResilienceProjection(
    val accountGeneration: Long,
    val accountEpoch: Long,
    val accountIdentity: String?,
    val transport: RootConnectionTransportState,
    val phase: RootConnectionPhase,
    val hasReachedBox: Boolean,
    val isPrivacyBlocked: Boolean,
    val failureCode: String?,
    val isRetrying: Boolean,
    val retryQueued: Boolean,
) {
    companion object {
        fun initial() = RootConnectionResilienceProjection(
            accountGeneration = 0L,
            accountEpoch = 0L,
            accountIdentity = null,
            transport = RootConnectionTransportState.CONNECTING,
            phase = RootConnectionPhase.HIDDEN,
            hasReachedBox = false,
            isPrivacyBlocked = false,
            failureCode = null,
            isRetrying = false,
            retryQueued = false,
        )
    }
}

/** Immutable projection only; canonical account/payment/entitlement/remote truth stays with domain owners. */
data class AccountAccessProjection(
    val accountEpoch: Long,
    val loggedIn: Boolean,
    val sandAccessState: AccountAccessState,
    val blockReason: AccountAccessBlockReason,
    val authorizationState: AccountTruthState,
    val paymentState: AccountPaymentState,
    val entitlementState: AccountEntitlementState,
    val entitlementReason: String?,
    val privacyMode: String,
    val teamPolicyState: AccountTruthState,
    val remoteReady: Boolean,
    val remoteHasDesktop: Boolean,
    val boxReady: Boolean,
    val sessionSettled: Boolean,
    val rebuildState: AccountRebuildState,
    val recoveryState: AccountRecoveryState,
    val rosterLoadState: AccountRosterLoadState,
    val rosterFailureCode: String?,
    val rosterFailureTransportKind: String?,
    val isShowingRestoredRoster: Boolean,
    val isRosterFetching: Boolean,
    val hasReachedBox: Boolean,
    val isAwaitingFirstBox: Boolean,
    val complete: Boolean,
    val detail: String? = null,
) {
    val explicitlyBlocked: Boolean
        get() = authorizationState == AccountTruthState.DENIED ||
            sandAccessState == AccountAccessState.UNAVAILABLE ||
            sandAccessState == AccountAccessState.PAYMENT_REQUIRED

    val mayShowAccessNotice: Boolean
        get() = loggedIn &&
            rosterFailureCode == "sand-access-blocked" &&
            !hasReachedBox &&
            !isShowingRestoredRoster &&
            rebuildState == AccountRebuildState.IDLE

    companion object {
        fun initial(accountEpoch: Long = 0L) = AccountAccessProjection(
            accountEpoch = accountEpoch,
            loggedIn = false,
            sandAccessState = AccountAccessState.UNKNOWN,
            blockReason = AccountAccessBlockReason.UNSPECIFIED,
            authorizationState = AccountTruthState.UNKNOWN,
            paymentState = AccountPaymentState.UNKNOWN,
            entitlementState = AccountEntitlementState.UNKNOWN,
            entitlementReason = null,
            privacyMode = "unknown",
            teamPolicyState = AccountTruthState.UNKNOWN,
            remoteReady = false,
            remoteHasDesktop = false,
            boxReady = false,
            sessionSettled = false,
            rebuildState = AccountRebuildState.UNKNOWN,
            recoveryState = AccountRecoveryState.UNKNOWN,
            rosterLoadState = AccountRosterLoadState.UNKNOWN,
            rosterFailureCode = null,
            rosterFailureTransportKind = null,
            isShowingRestoredRoster = false,
            isRosterFetching = false,
            hasReachedBox = false,
            isAwaitingFirstBox = false,
            complete = false,
        )
    }
}

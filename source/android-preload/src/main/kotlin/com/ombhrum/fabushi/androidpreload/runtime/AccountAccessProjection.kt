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
    val complete: Boolean,
    val detail: String? = null,
) {
    val explicitlyBlocked: Boolean
        get() = authorizationState == AccountTruthState.DENIED ||
            sandAccessState == AccountAccessState.UNAVAILABLE ||
            sandAccessState == AccountAccessState.PAYMENT_REQUIRED

    val mayShowAccessNotice: Boolean
        get() = loggedIn && explicitlyBlocked && rebuildState == AccountRebuildState.IDLE

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
            complete = false,
        )
    }
}

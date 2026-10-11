package com.ombhrum.fabushi

import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection
import com.ombhrum.fabushi.androidpreload.runtime.AccountRosterLoadState
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionResilienceProjection
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionTransportState

internal enum class RosterTransportState { BROWSER, CONNECTING, CONNECTED, DOWN }
internal enum class RosterLoadState { LOADING, READY, ERROR }

internal data class RosterFailureSnapshot(
    val code: String,
    val transportKind: String? = null,
)

internal data class RosterAccessReadinessInput(
    val accountKey: String?,
    val transport: RosterTransportState,
    val loadState: RosterLoadState,
    val hasLoadedAgents: Boolean,
    val agentIds: List<String>,
    val selectedAgentId: String?,
    val failure: RosterFailureSnapshot?,
    val isShowingRestoredRoster: Boolean,
    val isPrivacyBlocked: Boolean,
)

internal data class RosterAccessReadiness(
    val accountKey: String?,
    val isAccountBound: Boolean,
    val isConnected: Boolean,
    val isLoaded: Boolean,
    val hasReachedBox: Boolean,
    val hasSelectedAgent: Boolean,
    val isSelectionReady: Boolean,
    val rosterFailureCode: String?,
    val rosterFailureTransportKind: String?,
    val isShowingRestoredRoster: Boolean,
    val isPrivacyBlocked: Boolean,
)

internal fun projectRosterFailure(
    code: String?,
    transportKind: String? = null,
): RosterFailureSnapshot? =
    code
        ?.takeIf(String::isNotBlank)
        ?.let { RosterFailureSnapshot(it, transportKind?.takeIf(String::isNotBlank)) }

internal fun selectRosterAccessReadiness(
    input: RosterAccessReadinessInput,
): RosterAccessReadiness {
    val isAccountBound = input.accountKey != null
    val hasSelectedAgent =
        isAccountBound &&
            input.selectedAgentId != null &&
            input.selectedAgentId in input.agentIds
    val isLoaded =
        isAccountBound &&
            input.hasLoadedAgents &&
            input.loadState == RosterLoadState.READY
    val isConnected =
        isAccountBound &&
            input.transport == RosterTransportState.CONNECTED
    return RosterAccessReadiness(
        accountKey = input.accountKey,
        isAccountBound = isAccountBound,
        isConnected = isConnected,
        isLoaded = isLoaded,
        hasReachedBox = isAccountBound && input.hasLoadedAgents,
        hasSelectedAgent = hasSelectedAgent,
        isSelectionReady = isLoaded && isConnected && hasSelectedAgent,
        rosterFailureCode = input.failure?.code,
        rosterFailureTransportKind = input.failure?.transportKind,
        isShowingRestoredRoster = input.isShowingRestoredRoster,
        isPrivacyBlocked = input.isPrivacyBlocked,
    )
}

/**
 * Production adapter from canonical Coordinator-owned projections into the recovered pure selector.
 * This creates no second account/transport/roster owner; the derived accountKey is epoch-scoped and
 * never persisted or used for authorization.
 */
internal fun selectRosterAccessReadiness(
    access: AccountAccessProjection,
    connection: RootConnectionResilienceProjection,
    agentIds: List<String>,
    selectedAgentId: String?,
): RosterAccessReadiness =
    selectRosterAccessReadiness(
        RosterAccessReadinessInput(
            accountKey = access.accountEpoch
                .takeIf { access.loggedIn && it > 0L }
                ?.let { "epoch:$it" },
            transport = when (connection.transport) {
                RootConnectionTransportState.CONNECTING -> RosterTransportState.CONNECTING
                RootConnectionTransportState.CONNECTED -> RosterTransportState.CONNECTED
                RootConnectionTransportState.DOWN -> RosterTransportState.DOWN
            },
            loadState = when (access.rosterLoadState) {
                AccountRosterLoadState.READY -> RosterLoadState.READY
                AccountRosterLoadState.ERROR -> RosterLoadState.ERROR
                AccountRosterLoadState.LOADING,
                AccountRosterLoadState.UNKNOWN -> RosterLoadState.LOADING
            },
            hasLoadedAgents = access.hasReachedBox,
            agentIds = agentIds,
            selectedAgentId = selectedAgentId,
            failure = projectRosterFailure(
                access.rosterFailureCode,
                access.rosterFailureTransportKind,
            ),
            isShowingRestoredRoster = access.isShowingRestoredRoster,
            isPrivacyBlocked = connection.isPrivacyBlocked,
        ),
    )


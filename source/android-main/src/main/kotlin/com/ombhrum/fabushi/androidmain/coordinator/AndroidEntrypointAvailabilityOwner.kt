package com.ombhrum.fabushi.androidmain.coordinator

import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointAvailability
import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointAvailabilitySnapshot
import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointId

/**
 * Process-scoped owner of Desktop recovered-entrypoint availability on Android.
 *
 * The owner never accepts presentation-owned roster truth. Its callers supply the current
 * Coordinator account epoch, a Host-observed sand_agent_network gate and the Host canonical
 * roster observation. Tokens fence concurrent refreshes and account switches. Activity recreation
 * only re-observes this process owner; process recreation starts fail-closed until Host facts are
 * observed again.
 */
internal class AndroidEntrypointAvailabilityOwner(
    private val processGeneration: Long,
) {
    internal data class RefreshToken(
        val processGeneration: Long,
        val accountEpoch: Long,
        val sequence: Long,
    )

    private var sequence = 0L
    private var snapshot = AndroidEntrypointAvailabilitySnapshot.initial(
        processGeneration = processGeneration,
    )

    @Synchronized
    fun observeAccount(accountEpoch: Long): AndroidEntrypointAvailabilitySnapshot {
        require(accountEpoch >= 0L) { "entrypoint account epoch must be non-negative" }
        if (snapshot.accountEpoch != accountEpoch) {
            sequence += 1
            snapshot = AndroidEntrypointAvailabilitySnapshot.initial(
                processGeneration = processGeneration,
                accountEpoch = accountEpoch,
            )
        }
        return snapshot
    }

    @Synchronized
    fun begin(accountEpoch: Long): RefreshToken {
        observeAccount(accountEpoch)
        sequence += 1
        return RefreshToken(
            processGeneration = processGeneration,
            accountEpoch = accountEpoch,
            sequence = sequence,
        )
    }

    @Synchronized
    fun settle(
        token: RefreshToken,
        currentAccountEpoch: Long,
        sandAgentNetworkGateObserved: Boolean,
        sandAgentNetworkGateEnabled: Boolean,
        rosterObserved: Boolean,
        canonicalAgentCount: Int,
    ): AndroidEntrypointAvailabilitySnapshot {
        require(canonicalAgentCount >= 0) { "canonical agent count must be non-negative" }
        if (
            token.processGeneration != processGeneration ||
            token.accountEpoch != currentAccountEpoch ||
            token.sequence != sequence
        ) {
            return observeAccount(currentAccountEpoch)
        }

        val agentNetwork = when {
            !sandAgentNetworkGateObserved || !sandAgentNetworkGateEnabled ->
                AndroidEntrypointAvailability.Unavailable("gate")
            !rosterObserved ->
                AndroidEntrypointAvailability.Unavailable("roster")
            canonicalAgentCount == 0 ->
                AndroidEntrypointAvailability.Retained("empty-roster")
            else ->
                AndroidEntrypointAvailability.Available
        }
        snapshot = AndroidEntrypointAvailabilitySnapshot(
            accountEpoch = currentAccountEpoch,
            processGeneration = processGeneration,
            entries = mapOf(
                AndroidEntrypointId.COMPUTER to AndroidEntrypointAvailability.Available,
                AndroidEntrypointId.HIDDEN_CHATS to AndroidEntrypointAvailability.Available,
                AndroidEntrypointId.AGENT_NETWORK to agentNetwork,
                AndroidEntrypointId.PLUGINS to AndroidEntrypointAvailability.Available,
                AndroidEntrypointId.SETTINGS to AndroidEntrypointAvailability.Available,
            ),
        )
        return snapshot
    }

    @Synchronized
    fun clearForLoggedOutAccount(accountEpoch: Long): AndroidEntrypointAvailabilitySnapshot {
        sequence += 1
        snapshot = AndroidEntrypointAvailabilitySnapshot.initial(
            processGeneration = processGeneration,
            accountEpoch = accountEpoch,
        )
        return snapshot
    }
}

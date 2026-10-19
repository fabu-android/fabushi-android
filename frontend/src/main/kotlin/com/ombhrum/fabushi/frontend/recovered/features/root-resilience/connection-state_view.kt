package com.ombhrum.fabushi

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionPhase
import com.ombhrum.fabushi.androidpreload.runtime.RootConnectionResilienceProjection

/**
 * Native projection host for Desktop root-resilience/connection-state.tsx.
 *
 * It owns no transport/account state. It only renders the Coordinator-owned immutable
 * [RootConnectionResilienceProjection] and delegates retry intent back to that owner.
 */
@Composable
internal fun RootConnectionResilienceHost(
    projection: RootConnectionResilienceProjection,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    when (projection.phase) {
        RootConnectionPhase.HIDDEN,
        RootConnectionPhase.CONNECTED -> Unit

        RootConnectionPhase.LOADING ->
            RosterStatus(
                kind = RosterStatusKind.LOADING,
                modifier = modifier,
            )

        RootConnectionPhase.RECONNECTING ->
            RosterReconnectNotice(
                isRetrying = projection.isRetrying,
                onRetry = onRetry,
                modifier = modifier,
            )

        RootConnectionPhase.UNREACHABLE ->
            RosterStatus(
                kind = RosterStatusKind.ERROR,
                isRetrying = projection.isRetrying,
                onRetry = onRetry,
                modifier = modifier,
            )
    }
}

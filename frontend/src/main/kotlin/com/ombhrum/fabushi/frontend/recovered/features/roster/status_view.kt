package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

internal enum class RosterStatusKind { LOADING, EMPTY, ALL_HIDDEN, ERROR }

@Composable
internal fun RosterStatus(
    kind: RosterStatusKind,
    isRetrying: Boolean = false,
    onRetry: (() -> Unit)? = null,
    onShowHiddenBots: (() -> Unit)? = null,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier.testTag("roster-status-${kind.name.lowercase()}"),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        when (kind) {
            RosterStatusKind.LOADING -> Text("Connecting to your computer…")
            RosterStatusKind.EMPTY -> Text("No saved agents yet.")
            RosterStatusKind.ALL_HIDDEN -> {
                Text("All bots are hidden")
                Button(
                    onClick = { onShowHiddenBots?.invoke() },
                    enabled = onShowHiddenBots != null,
                    modifier = Modifier.testTag("roster-show-hidden"),
                ) {
                    Text("Show Hidden Bots")
                }
            }
            RosterStatusKind.ERROR -> {
                Text("Can’t reach your computer")
                Text("Your agents are safe — they just can’t be loaded right now.")
                Button(
                    onClick = { onRetry?.invoke() },
                    enabled = onRetry != null && !isRetrying,
                    modifier = Modifier.testTag("roster-retry"),
                ) {
                    Text(if (isRetrying) "Retrying…" else "Retry")
                }
            }
        }
    }
}

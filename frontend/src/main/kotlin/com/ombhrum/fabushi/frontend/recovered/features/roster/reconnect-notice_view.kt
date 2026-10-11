package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag

@Composable
internal fun RosterReconnectNotice(
    isRetrying: Boolean,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier
            .fillMaxWidth()
            .testTag("roster-reconnect-notice"),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("Reconnecting to your computer…")
        Button(
            onClick = onRetry,
            enabled = !isRetrying,
            modifier = Modifier.testTag("roster-reconnect-retry"),
        ) {
            Text(if (isRetrying) "Retrying…" else "Retry")
        }
    }
}

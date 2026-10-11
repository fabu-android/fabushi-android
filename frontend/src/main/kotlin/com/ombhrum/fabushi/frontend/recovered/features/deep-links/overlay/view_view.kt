package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp

@Composable
internal fun DeepLinkInfoDialog(
    link: DeepLinkInfo?,
    onClose: () -> Unit,
) {
    val active = link ?: return
    AlertDialog(
        onDismissRequest = onClose,
        modifier = Modifier.testTag("deep-link-info-dialog"),
        title = { Text("Deep Links") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Fabushi deep links are working")
                Column {
                    Text("Route")
                    Text(
                        deepLinkRoute(active),
                        modifier = Modifier.testTag("deep-link-info-route"),
                    )
                }
                Column {
                    Text("Source")
                    Text(
                        deepLinkSourceLabel(active.source),
                        modifier = Modifier.testTag("deep-link-info-source"),
                    )
                }
            }
        },
        confirmButton = {
            Button(
                onClick = onClose,
                modifier = Modifier.testTag("deep-link-info-done"),
            ) {
                Text("Done")
            }
        },
        dismissButton = {
            TextButton(
                onClick = onClose,
                modifier = Modifier.testTag("deep-link-info-close"),
            ) {
                Text("Close")
            }
        },
    )
}

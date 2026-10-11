package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

/**
 * Native Compose presentation for Desktop app-alert/view.tsx.
 *
 * The controller owns only ephemeral modal coordination. Domain/canonical business state remains
 * with its existing Host/Coordinator owner and is supplied through request actions.
 */
@Composable
internal fun AppAlertHost(
    controller: AppAlertController,
    modifier: Modifier = Modifier,
) {
    var state by remember(controller) { mutableStateOf(controller.getSnapshot()) }
    val focusRequester = remember { FocusRequester() }

    DisposableEffect(controller) {
        val subscription = controller.subscribe {
            state = controller.getSnapshot()
        }
        onDispose { subscription.close() }
    }

    val current = state ?: return
    val request = current.request

    LaunchedEffect(request, current.isPerforming) {
        if (!current.isPerforming) {
            runCatching { focusRequester.requestFocus() }
        }
    }

    AlertDialog(
        modifier = modifier
            .widthIn(max = if (request.wide) 500.dp else 380.dp)
            .testTag("app-alert"),
        onDismissRequest = {
            if (!current.isPerforming) controller.cancel()
        },
        title = { Text(request.title) },
        text = {
            Column {
                request.description?.let { Text(it) }
                request.body?.let { Text(it) }
                request.warning?.let { Text(it) }
                current.failure?.let {
                    Text(
                        text = it,
                        modifier = Modifier
                            .semantics { liveRegion = LiveRegionMode.Assertive }
                            .testTag("app-alert-failure"),
                    )
                }
            }
        },
        confirmButton = {
            Button(
                modifier = Modifier
                    .focusRequester(focusRequester)
                    .testTag(if (request.destructive) "app-alert-confirm-destructive" else "app-alert-confirm"),
                enabled = !current.isPerforming,
                onClick = controller::confirm,
            ) {
                Text(
                    if (current.isPerforming) {
                        request.pendingLabel ?: request.confirmLabel
                    } else {
                        request.confirmLabel
                    },
                )
            }
        },
        dismissButton = {
            Column {
                request.secondary?.let { secondary ->
                    TextButton(
                        modifier = Modifier.testTag(
                            if (secondary.destructive) "app-alert-secondary-destructive"
                            else "app-alert-secondary",
                        ),
                        enabled = !current.isPerforming,
                        onClick = controller::confirmSecondary,
                    ) {
                        Text(secondary.label)
                    }
                }
                request.cancelLabel?.let { label ->
                    TextButton(
                        modifier = Modifier.testTag("app-alert-cancel"),
                        enabled = !current.isPerforming,
                        onClick = controller::cancel,
                    ) {
                        Text(label)
                    }
                }
            }
        },
    )
}

package com.ombhrum.fabushi

import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import kotlinx.coroutines.launch

internal data class AgentDeleteTarget(
    val id: String,
    val name: String,
    val isGroup: Boolean = false,
)

internal fun agentDeleteDescription(agent: AgentDeleteTarget): String =
    if (agent.isGroup) {
        "This permanently deletes the group and its chat history. The Bots in it are not deleted and remain available individually. This can\'t be undone."
    } else {
        "This permanently deletes the agent and its chat history. This can\'t be undone."
    }

/**
 * Android/Compose counterpart of Grok\'s destructive single-agent confirmation.
 *
 * The dialog stays open while Host deletion is pending and stays open on failure. That prevents
 * renderer optimism from diverging from Coordinator/Host truth.
 */
@Composable
internal fun AgentDeleteConfirmation(
    agent: AgentDeleteTarget?,
    onClose: () -> Unit,
    onConfirm: suspend (String) -> Unit,
) {
    if (agent == null) return
    var pending by remember(agent.id) { mutableStateOf(false) }
    var failure by remember(agent.id) { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()

    LaunchedEffect(agent.id) {
        pending = false
        failure = null
    }

    AlertDialog(
        onDismissRequest = {
            if (!pending) onClose()
        },
        title = { Text("Delete “${agent.name}”") },
        text = {
            Text(
                buildString {
                    append(agentDeleteDescription(agent))
                    failure?.let {
                        append("\n\n")
                        append(it)
                    }
                },
                modifier = Modifier.testTag(
                    if (failure == null) "agent-delete-description" else "agent-delete-error",
                ),
            )
        },
        confirmButton = {
            TextButton(
                enabled = !pending,
                modifier = Modifier.testTag("agent-delete-confirm"),
                onClick = {
                    if (pending) return@TextButton
                    pending = true
                    failure = null
                    scope.launch {
                        runCatching { onConfirm(agent.id) }
                            .onSuccess { onClose() }
                            .onFailure {
                                failure = "Deleting failed. Check your connection and try again."
                            }
                        pending = false
                    }
                },
            ) {
                Text(if (pending) "Deleting..." else "Delete")
            }
        },
        dismissButton = {
            TextButton(
                enabled = !pending,
                onClick = onClose,
                modifier = Modifier.testTag("agent-delete-cancel"),
            ) {
                Text("Cancel")
            }
        },
        modifier = Modifier.testTag("agent-delete-dialog"),
    )
}

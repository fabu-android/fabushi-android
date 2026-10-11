package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

internal const val ASYNC_TASKS_REFRESH_INTERVAL_MS = 30_000L

internal fun formatAsyncTaskTime(timestampMs: Long, nowMs: Long): String {
    if (timestampMs <= 0L || nowMs < 0L) return ""
    val totalSeconds = ((nowMs - timestampMs).coerceAtLeast(0L)) / 1_000L
    if (totalSeconds < 60L) return "now"
    val minutes = totalSeconds / 60L
    if (minutes < 60L) return "${minutes}m ago"
    val hours = minutes / 60L
    if (hours < 24L) return "${hours}h ago"
    val days = hours / 24L
    if (days < 30L) return "${days}d ago"
    val months = days / 30L
    if (months < 12L) return "${months}mo ago"
    return "${months / 12L}y ago"
}

internal fun asyncTaskMeta(task: MobileAsyncTask): String {
    val kind = when (task.kind) {
        "subagent" -> "Subagent"
        "shell" -> "Shell"
        "cloud-agent" -> "Cloud agent"
        else -> task.kind
    }
    val detail = task.detail?.trim().orEmpty()
    return if (detail.isEmpty()) kind else "$kind · $detail"
}

@Composable
internal fun AgentAsyncTasksPanel(
    agent: MobileBotSummaryAndroid,
    tasks: List<MobileAsyncTask>,
    loading: Boolean,
    error: String?,
    onClose: () -> Unit,
    onRefresh: () -> Unit,
) {
    var nowMs by remember(agent.id) { mutableLongStateOf(System.currentTimeMillis()) }
    LaunchedEffect(agent.id) {
        while (true) {
            delay(ASYNC_TASKS_REFRESH_INTERVAL_MS)
            nowMs = System.currentTimeMillis()
            onRefresh()
        }
    }

    AlertDialog(
        onDismissRequest = onClose,
        title = { Text("Async tasks: ${agent.name}") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                when {
                    loading && tasks.isEmpty() -> CircularProgressIndicator(
                        modifier = Modifier.testTag("agent-async-tasks-loading"),
                    )
                    error != null && tasks.isEmpty() -> Text(
                        error,
                        modifier = Modifier.testTag("agent-async-tasks-error"),
                    )
                    tasks.isEmpty() -> Text(
                        "No async tasks in progress.",
                        modifier = Modifier.testTag("agent-async-tasks-empty"),
                    )
                    else -> LazyColumn(
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                        modifier = Modifier.testTag("agent-async-tasks-list"),
                    ) {
                        items(tasks, key = { "${it.kind}:${it.id}" }) { task ->
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                            ) {
                                Column(modifier = Modifier.weight(1f)) {
                                    Text(task.label)
                                    Text(asyncTaskMeta(task))
                                }
                                Text(formatAsyncTaskTime(task.startedAtMs, nowMs))
                            }
                        }
                    }
                }
                if (error != null && tasks.isNotEmpty()) {
                    Text(
                        "Refresh failed: $error",
                        modifier = Modifier.testTag("agent-async-tasks-stale-error"),
                    )
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onRefresh, enabled = !loading) {
                Text(if (loading) "Refreshing…" else "Refresh")
            }
        },
        dismissButton = {
            TextButton(onClick = onClose) { Text("Close") }
        },
        modifier = Modifier.testTag("agent-async-tasks-dialog"),
    )
}

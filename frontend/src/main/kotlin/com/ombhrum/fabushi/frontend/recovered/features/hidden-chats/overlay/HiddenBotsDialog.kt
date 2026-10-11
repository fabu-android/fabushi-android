package com.ombhrum.fabushi

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.weight
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

internal data class HiddenBotRowProjection(
    val id: String,
    val name: String,
)

internal fun projectHiddenBotRows(
    bots: List<MobileBotSummaryAndroid>,
): List<HiddenBotRowProjection> =
    bots.asSequence()
        .filter { it.isHidden }
        .map { HiddenBotRowProjection(id = it.id, name = it.name) }
        .distinctBy(HiddenBotRowProjection::id)
        .toList()

internal fun hiddenChatsEmptyLabel(rows: List<HiddenBotRowProjection>): String? =
    if (rows.isEmpty()) "No hidden bots" else null

/**
 * Native Android replacement for the Desktop Hidden Bots chooser.
 *
 * Canonical hidden state stays in the Rust Agent roster. This surface only projects current
 * hidden rows and emits explicit open/unhide intents through the typed ViewModel/Coordinator path.
 */
@Composable
internal fun HiddenBotsDialog(
    open: Boolean,
    hiddenBots: List<MobileBotSummaryAndroid>,
    onClose: () -> Unit,
    onOpenBot: (MobileBotSummaryAndroid) -> Unit,
    onUnhide: (String) -> Unit,
) {
    if (!open) return
    val rows = projectHiddenBotRows(hiddenBots)
    val byId = hiddenBots.associateBy(MobileBotSummaryAndroid::id)
    AlertDialog(
        onDismissRequest = onClose,
        modifier = Modifier.testTag("hidden-bots-dialog"),
        title = {
            Column {
                Text(
                    "Hidden Bots",
                    modifier = Modifier.semantics { heading() },
                )
                Text("Hidden Bots stay active and keep their history, they just don't show in the sidebar.")
            }
        },
        text = {
            val empty = hiddenChatsEmptyLabel(rows)
            if (empty != null) {
                Text(empty, modifier = Modifier.testTag("hidden-bots-empty"))
            } else {
                LazyColumn(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(max = 520.dp)
                        .testTag("hidden-bots-list"),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    items(rows, key = HiddenBotRowProjection::id) { row ->
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            TextButton(
                                onClick = {
                                    byId[row.id]?.let(onOpenBot)
                                },
                                modifier = Modifier
                                    .weight(1f)
                                    .testTag("hidden-bot-open-" + row.id),
                            ) {
                                Text(row.name, modifier = Modifier.fillMaxWidth())
                            }
                            TextButton(
                                onClick = { onUnhide(row.id) },
                                modifier = Modifier.testTag("hidden-bot-unhide-" + row.id),
                            ) {
                                Text("Unhide")
                            }
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                onClick = onClose,
                modifier = Modifier.testTag("hidden-bots-close"),
            ) {
                Text("Close")
            }
        },
    )
}

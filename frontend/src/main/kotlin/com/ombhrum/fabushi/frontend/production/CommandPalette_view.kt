package com.ombhrum.fabushi

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

@Composable
internal fun CommandPalette(
    open: Boolean,
    entries: List<CommandPaletteEntry>,
    messageSearchStatus: CommandPaletteMessageStatus = CommandPaletteMessageStatus.IDLE,
    routineStatus: CommandPaletteRoutineStatus = CommandPaletteRoutineStatus.IDLE,
    initialTab: CommandPaletteTab = CommandPaletteTab.ALL,
    onQueryChange: (String) -> Unit = {},
    onDismiss: () -> Unit,
) {
    if (!open) return
    var query by remember(open) { mutableStateOf("") }
    var tab by remember(open, initialTab) { mutableStateOf(initialTab) }
    val visible = remember(entries, query, tab) {
        commandPaletteEntries(entries, tab, query)
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Search") },
        text = {
            Column(
                verticalArrangement = Arrangement.spacedBy(8.dp),
                modifier = Modifier.fillMaxWidth(),
            ) {
                OutlinedTextField(
                    value = query,
                    onValueChange = {
                        query = it
                        onQueryChange(it)
                    },
                    singleLine = true,
                    placeholder = { Text("Search agents and actions") },
                    modifier = Modifier
                        .fillMaxWidth()
                        .testTag("command-palette-search"),
                )
                Row(
                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    listOf(
                        CommandPaletteTab.ALL to "All",
                        CommandPaletteTab.MESSAGES to "Messages",
                        CommandPaletteTab.AGENTS to "Agents",
                        CommandPaletteTab.GROUPS to "Groups",
                        CommandPaletteTab.ROUTINES to "Routines",
                        CommandPaletteTab.ACTIONS to "Actions",
                    ).forEach { (candidate, label) ->
                        TextButton(onClick = { tab = candidate }) {
                            Text(
                                label,
                                fontWeight = if (tab == candidate) FontWeight.Bold else FontWeight.Normal,
                            )
                        }
                    }
                }
                if (visible.isEmpty()) {
                    val emptyMessage = when {
                        tab == CommandPaletteTab.MESSAGES && query.isBlank() -> "Search messages"
                        tab == CommandPaletteTab.ROUTINES &&
                            routineStatus == CommandPaletteRoutineStatus.LOADING -> "Loading routines…"
                        tab == CommandPaletteTab.ROUTINES &&
                            routineStatus == CommandPaletteRoutineStatus.FAILED -> "Routines unavailable"
                        tab == CommandPaletteTab.ROUTINES &&
                            routineStatus == CommandPaletteRoutineStatus.EMPTY -> "No routines"
                        messageSearchStatus == CommandPaletteMessageStatus.LOADING &&
                            (tab == CommandPaletteTab.MESSAGES || tab == CommandPaletteTab.ALL) -> "Searching messages…"
                        messageSearchStatus == CommandPaletteMessageStatus.FAILED &&
                            (tab == CommandPaletteTab.MESSAGES || tab == CommandPaletteTab.ALL) -> "Message search unavailable"
                        else -> "No matching results"
                    }
                    Text(
                        emptyMessage,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(vertical = 18.dp),
                    )
                } else {
                    LazyColumn(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(max = 360.dp)
                            .testTag("command-palette-results"),
                    ) {
                        itemsIndexed(visible, key = { _, entry -> entry.id }) { index, entry ->
                            Column(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clickable {
                                        if (activateCommandPaletteEntry(visible, index)) onDismiss()
                                    }
                                    .padding(horizontal = 6.dp, vertical = 10.dp)
                                    .testTag("command-palette-entry-${entry.id}"),
                            ) {
                                Text(
                                    entry.label,
                                    fontWeight = FontWeight.SemiBold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                entry.detail?.takeIf(String::isNotBlank)?.let {
                                    Text(
                                        it,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text("Close") }
        },
        modifier = Modifier.testTag("command-palette"),
    )
}

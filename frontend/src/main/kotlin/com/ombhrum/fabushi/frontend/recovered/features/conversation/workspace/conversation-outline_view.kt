package com.ombhrum.fabushi

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.weight
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import com.ombhrum.fabushi.androidpreload.runtime.AndroidConversationOutlineItem

internal fun conversationOutlineItemLabel(item: AndroidConversationOutlineItem): String =
    when (item.kind) {
        "user" -> "You"
        "thinking" -> "Thinking"
        "assistant-text" -> "Agent"
        "send-message" -> "Message"
        "tool-call" -> item.name
            ?.removeSuffix("ToolCall")
            ?.replace(Regex("([a-z0-9])([A-Z])"), "$1 $2")
            ?.replaceFirstChar { if (it.isLowerCase()) it.titlecase() else it.toString() }
            ?: "Tool"
        else -> "Activity"
    }

internal fun conversationOutlineItemPreview(item: AndroidConversationOutlineItem): String =
    when (item.kind) {
        "tool-call" -> item.summary.orEmpty()
        "send-message" -> item.messageValue.orEmpty()
        else -> item.text.orEmpty()
    }.trim().lineSequence().firstOrNull().orEmpty()

@Composable
internal fun ConversationOutlinePanelAndroid(
    state: MobileConversationOutlineState,
    onSelectAgent: (String) -> Unit,
    onRefresh: () -> Unit,
    onClose: () -> Unit,
) {
    val parentId = state.parentAgentId ?: return
    val selectedId = state.selectedAgentId ?: parentId
    var expandedIds by remember(parentId, selectedId) { mutableStateOf(emptySet<String>()) }

    Dialog(
        onDismissRequest = onClose,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Surface(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 18.dp, vertical = 32.dp)
                .testTag("conversation-outline-panel"),
            shape = RoundedCornerShape(22.dp),
        ) {
            Column(Modifier.fillMaxSize().padding(16.dp)) {
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text("Full conversation", fontWeight = FontWeight.Bold, fontSize = 18.sp)
                        Text(
                            state.parentAgentName.ifBlank { "Conversation" },
                            color = GrokMobileMuted,
                            fontSize = 12.sp,
                        )
                    }
                    TextButton(onClick = onRefresh, enabled = !state.loading) { Text("Refresh") }
                    TextButton(
                        onClick = onClose,
                        modifier = Modifier.testTag("conversation-outline-close"),
                    ) { Text("Close") }
                }

                val tabs = buildList {
                    add(Triple(parentId, state.parentAgentName.ifBlank { "Conversation" }, ""))
                    state.subagents.forEach { child ->
                        add(Triple(child.id, "${child.type}: ${child.title}".take(120), child.status))
                    }
                }
                if (tabs.size > 1) {
                    LazyRow(
                        modifier = Modifier.fillMaxWidth().padding(vertical = 8.dp),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        items(tabs, key = { it.first }) { (id, label, status) ->
                            val selected = id == selectedId
                            Text(
                                buildString {
                                    if (status == "running") append("● ")
                                    append(label)
                                },
                                color = if (selected) GrokMobileInk else GrokMobileMuted,
                                fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
                                modifier = Modifier
                                    .background(
                                        if (selected) GrokMobileInk.copy(alpha = 0.08f)
                                        else GrokMobileBackground,
                                        RoundedCornerShape(18.dp),
                                    )
                                    .clickable { onSelectAgent(id) }
                                    .padding(horizontal = 12.dp, vertical = 8.dp)
                                    .testTag("conversation-outline-tab-$id"),
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                }

                when {
                    state.loading && state.items.isEmpty() -> Row(
                        Modifier.fillMaxWidth().padding(top = 36.dp),
                        horizontalArrangement = Arrangement.Center,
                    ) { CircularProgressIndicator() }
                    state.error != null && state.items.isEmpty() -> Column(
                        Modifier.fillMaxWidth().padding(top = 28.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(state.error, color = GrokMobileMuted)
                        Button(onClick = onRefresh) { Text("Retry") }
                    }
                    state.items.isEmpty() -> Text(
                        "No conversation activity yet.",
                        color = GrokMobileMuted,
                        modifier = Modifier.fillMaxWidth().padding(top = 32.dp),
                    )
                    else -> LazyColumn(
                        Modifier.fillMaxWidth().weight(1f),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        items(state.items, key = { it.id }) { item ->
                            val expanded = item.id in expandedIds
                            val preview = conversationOutlineItemPreview(item)
                            Column(
                                Modifier
                                    .fillMaxWidth()
                                    .background(GrokMobileBackground, RoundedCornerShape(12.dp))
                                    .clickable {
                                        expandedIds = if (expanded) expandedIds - item.id
                                        else expandedIds + item.id
                                    }
                                    .padding(horizontal = 12.dp, vertical = 10.dp)
                                    .testTag("conversation-outline-item-${item.id}"),
                            ) {
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text(
                                        conversationOutlineItemLabel(item),
                                        fontWeight = FontWeight.SemiBold,
                                        modifier = Modifier.width(104.dp),
                                    )
                                    Text(
                                        preview.ifBlank {
                                            if (item.kind == "tool-call") item.status.orEmpty() else ""
                                        },
                                        color = GrokMobileMuted,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                        modifier = Modifier.weight(1f),
                                    )
                                    Text(if (expanded) "−" else "+", color = GrokMobileMuted)
                                }
                                if (expanded) {
                                    Text(
                                        preview.ifBlank { "No additional details." },
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(top = 8.dp)
                                            .heightIn(max = 240.dp),
                                    )
                                }
                            }
                        }
                    }
                }
                state.error?.takeIf { state.items.isNotEmpty() }?.let {
                    Text(it, color = GrokMobileMuted, fontSize = 11.sp, modifier = Modifier.padding(top = 4.dp))
                }
            }
        }
    }
}

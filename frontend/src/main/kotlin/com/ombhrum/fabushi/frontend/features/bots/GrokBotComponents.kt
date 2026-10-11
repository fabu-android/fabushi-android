package com.ombhrum.fabushi

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlin.math.absoluteValue
import kotlin.math.sin


@Composable
internal fun SectionLabelAndroid(text: String) {
    Text(text, color = GrokMobileInk.copy(alpha = 0.42f), fontSize = 16.sp, modifier = Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 8.dp))
}

@Composable
private fun MobileToolResultTranscriptCard(card: MobileToolResultCard) {
    val title = when (card.kind) {
        "file-edit" -> "Edited file"
        "file-write" -> "Wrote file"
        "shell" -> if (card.isBackground) "Terminal · background" else "Terminal"
        else -> "Tool result"
    }
    Column(
        Modifier
            .fillMaxWidth()
            .background(Color(0xFFF6F6F7), RoundedCornerShape(14.dp))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(title, color = GrokMobileInk, fontWeight = FontWeight.SemiBold)
            Text(
                card.status,
                color = when (card.status) {
                    "error", "denied", "rejected", "cancelled" -> Color.Red
                    "running", "background" -> Color(0xFFE18900)
                    else -> GrokMobileMuted
                },
                fontSize = 11.sp,
            )
        }
        card.path?.let { Text(it, color = GrokMobileMuted, fontSize = 12.sp) }
        card.command?.let { Text("$ $it", color = GrokMobileInk, fontSize = 12.sp) }
        card.workingDirectory?.let { Text(it, color = GrokMobileMuted, fontSize = 11.sp) }
        if (card.summary.isNotBlank()) {
            Text(card.summary, color = GrokMobileInk, fontSize = 12.sp)
        }
        if (card.diff.isNotBlank()) {
            Text(card.diff, color = GrokMobileInk, fontSize = 11.sp)
        }
        if (card.output.isNotBlank()) {
            Text(card.output, color = GrokMobileInk, fontSize = 11.sp)
        }
        if (card.isStreaming) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                CircularProgressIndicator(Modifier.size(12.dp), strokeWidth = 1.5.dp)
                Text("Streaming…", color = GrokMobileMuted, fontSize = 11.sp)
            }
        }
    }
}

@Composable
private fun MobileWidgetTranscriptCard(
    entry: MobileChatMessage,
    pending: Boolean,
    onRespond: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val widget = entry.widget ?: return
    var custom by remember(entry.id) { mutableStateOf("") }
    val settled = widget.respondedValue != null || widget.dismissed
    Column(
        Modifier
            .fillMaxWidth()
            .background(Color(0xFFF6F6F7), RoundedCornerShape(14.dp))
            .padding(12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f)) {
                Text(widget.prompt, color = GrokMobileInk, fontWeight = FontWeight.SemiBold)
                widget.helpText?.let { Text(it, color = GrokMobileMuted, fontSize = 12.sp) }
            }
            if (!settled) {
                TextButton(onClick = onDismiss, enabled = !pending) { Text("关闭") }
            }
        }
        when {
            widget.dismissed -> Text("已忽略", color = GrokMobileMuted, fontSize = 12.sp)
            widget.respondedValue != null -> {
                val label = widget.options.firstOrNull { it.value == widget.respondedValue }?.label
                    ?: widget.respondedValue
                Text("✓ $label", color = GrokMobileInk, fontWeight = FontWeight.Medium)
            }
            else -> {
                widget.options.forEachIndexed { index, option ->
                    Button(
                        onClick = { onRespond(option.value) },
                        enabled = !pending,
                        modifier = Modifier.fillMaxWidth(),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = Color.White,
                            contentColor = GrokMobileInk,
                        ),
                    ) {
                        Column(Modifier.fillMaxWidth()) {
                            Text("${('A'.code + index).toChar()}. ${option.label}")
                            option.description?.let {
                                Text(it, color = GrokMobileMuted, fontSize = 11.sp)
                            }
                        }
                    }
                }
                if (widget.allowCustom) {
                    OutlinedTextField(
                        value = custom,
                        onValueChange = { custom = it },
                        enabled = !pending,
                        modifier = Modifier.fillMaxWidth(),
                        placeholder = { Text("自定义回答") },
                        maxLines = 3,
                    )
                    if (custom.trim().isNotEmpty()) {
                        Button(
                            onClick = { onRespond(custom.trim()) },
                            enabled = !pending,
                        ) { Text(if (pending) "提交中…" else "提交") }
                    }
                }
                if (pending) {
                    Row(
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        CircularProgressIndicator(Modifier.size(12.dp), strokeWidth = 1.5.dp)
                        Text("提交中…", color = GrokMobileMuted, fontSize = 11.sp)
                    }
                }
            }
        }
    }
}


@Composable
private fun MobileTranscriptAttachments(
    attachments: List<MobileTranscriptAttachment>,
    alignEnd: Boolean,
) {
    if (attachments.isEmpty()) return
    Column(
        Modifier.fillMaxWidth(),
        horizontalAlignment = if (alignEnd) Alignment.End else Alignment.Start,
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        attachments.forEach { attachment ->
            Column(
                Modifier
                    .background(Color.Black.copy(alpha = if (alignEnd) 0.08f else 0.045f), RoundedCornerShape(12.dp))
                    .padding(horizontal = 10.dp, vertical = 7.dp)
                    .testTag("mobile-bot-attachment-" + attachment.name.replace(Regex("[^A-Za-z0-9._-]"), "-").take(120)),
            ) {
                Text(
                    attachment.name,
                    color = GrokMobileInk,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val metadata = buildList {
                    attachment.mimeType?.takeIf(String::isNotBlank)?.let { add(it) }
                    attachment.sizeBytes?.let { add(formatTranscriptAttachmentBytes(it)) }
                }.joinToString(" · ")
                if (metadata.isNotBlank()) {
                    Text(metadata, color = GrokMobileMuted, fontSize = 10.sp)
                }
            }
        }
    }
}

internal fun formatTranscriptAttachmentBytes(bytes: Long): String = when {
    bytes < 1024L -> "$bytes B"
    bytes < 1024L * 1024L -> {
        val tenths = (bytes * 10L + 512L) / 1024L
        if (tenths < 100L && tenths % 10L != 0L) "${tenths / 10L}.${tenths % 10L} KB" else "${(bytes + 512L) / 1024L} KB"
    }
    else -> {
        val tenths = (bytes * 10L + (1024L * 1024L) / 2L) / (1024L * 1024L)
        "${tenths / 10L}.${tenths % 10L} MB"
    }
}


internal fun mobileTranscriptBubbleShape(
    role: MobileChatRole,
    adjacency: TranscriptAdjacency,
): RoundedCornerShape {
    val large = 18.dp
    val seam = 6.dp
    return when (role) {
        MobileChatRole.USER -> RoundedCornerShape(
            topStart = large,
            topEnd = if (adjacency.isContinuedFromPrev) seam else large,
            bottomStart = large,
            bottomEnd = if (adjacency.isContinuedToNext) seam else large,
        )
        MobileChatRole.ASSISTANT -> RoundedCornerShape(
            topStart = if (adjacency.isContinuedFromPrev) seam else large,
            topEnd = large,
            bottomStart = if (adjacency.isContinuedToNext) seam else large,
            bottomEnd = large,
        )
    }
}

@Composable
internal fun GrokBotRowAndroid(
    bot: MobileBotSummaryAndroid,
    badge: String,
    onClick: (MobileBotSummaryAndroid) -> Unit,
    editingName: Boolean = false,
    onNameCommit: ((String) -> Unit)? = null,
    onNameExit: (() -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(enabled = !editingName) { onClick(bot) }
            .padding(horizontal = 18.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        AgentAvatarAndroid(
            bot = bot,
            size = 47.dp,
            badge = Color(0xFF20B967),
        )
        Column(Modifier.weight(1f).padding(start = 12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (editingName && onNameCommit != null && onNameExit != null) {
                    AgentNameEditor(
                        initialValue = bot.name,
                        onCommit = onNameCommit,
                        onExit = onNameExit,
                        modifier = Modifier.weight(1f),
                    )
                } else {
                    Text(
                        bot.name,
                        color = GrokMobileInk,
                        fontSize = 17.sp,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
                Text(
                    badge,
                    color = GrokMobileMuted,
                    fontSize = 11.sp,
                    modifier = Modifier
                        .padding(start = 7.dp)
                        .background(Color.Black.copy(alpha = 0.045f), RoundedCornerShape(20.dp))
                        .padding(horizontal = 7.dp, vertical = 3.dp),
                )
            }
            Text(bot.description.ifBlank { "Ready" }, color = GrokMobileMuted, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (trailing != null) {
            trailing()
        } else {
            Text("now", color = GrokMobileMuted, fontSize = 11.sp)
        }
    }
}

@Composable
internal fun GrokBotChatAndroid(
    bot: MobileBotSummaryAndroid,
    state: MobileBotUiState,
    appAgentSurface: FabushiAppAgentSurface,
    onClose: () -> Unit,
    onOpenCommandPalette: () -> Unit,
    onOpenSettings: () -> Unit,
    onOpenConversationOutline: () -> Unit,
    onDraftChange: (String) -> Unit,
    onAddDraftAttachments: (List<String>) -> Unit,
    onRemoveDraftAttachment: (String) -> Unit,
    onSend: () -> Unit,
    onRetrySend: (String) -> Unit,
    onCancelQueuedSend: (String) -> Unit,
    onRetryTranscriptLoad: () -> Unit,
    onLoadOlderMessages: () -> Unit,
    onRespondToWidget: (String, String) -> Unit,
    onDismissWidget: (String) -> Unit,
    onStop: () -> Unit,
    onResolveApproval: (Boolean) -> Unit,
    onMessageTargetConsumed: (String) -> Unit,
) {
    val messageListState = rememberLazyListState()
    var findOpen by remember(bot.id) { mutableStateOf(false) }
    var findQuery by remember(bot.id) { mutableStateOf("") }
    var findIndex by remember(bot.id) { mutableStateOf(-1) }
    var historyAnchor by remember(bot.id) { mutableStateOf<Pair<String, Int>?>(null) }
    val attachmentPicker = rememberLauncherForActivityResult(
        contract = ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris ->
        if (uris.isNotEmpty()) onAddDraftAttachments(uris.map { it.toString() })
    }
    val findMatches = remember(state.findEntries, findQuery) {
        findInCanonicalAgentChatMatches(state.findEntries, findQuery)
    }
    val currentFindMessageId = findMatches.getOrNull(findIndex)?.messageId
    val transcriptAdjacency = remember(state.messages, bot.name) {
        projectAgentTranscriptAdjacency(
            messages = state.messages,
            assistantAuthor = bot.name,
        )
    }

    LaunchedEffect(bot.id, findQuery, findMatches.size) {
        if (findIndex !in findMatches.indices) findIndex = -1
    }
    LaunchedEffect(bot.id, currentFindMessageId, state.messages) {
        val targetId = currentFindMessageId ?: return@LaunchedEffect
        val targetIndex = state.messages.indexOfFirst { it.id == targetId }
        if (targetIndex >= 0) messageListState.animateScrollToItem(targetIndex)
    }
    LaunchedEffect(bot.id, state.historyPrependRevision) {
        val anchor = historyAnchor ?: return@LaunchedEffect
        val index = state.messages.indexOfFirst { it.id == anchor.first }
        if (index >= 0) {
            messageListState.scrollToItem(index, anchor.second)
        }
        historyAnchor = null
    }

    LaunchedEffect(bot.id, state.messageTargetId, state.messages) {
        val targetId = state.messageTargetId ?: return@LaunchedEffect
        val targetIndex = state.messages.indexOfFirst { it.id == targetId }
        if (targetIndex < 0) return@LaunchedEffect
        messageListState.animateScrollToItem(targetIndex)
        onMessageTargetConsumed(targetId)
    }
    LaunchedEffect(bot.id, state.draft, state.busy, state.error, state.messages, state.pendingApproval, appAgentSurface) {
        val elements = mutableListOf(
            FabushiAppAgentSurface.Element("mobile-bot-chat", "application", "Bot ${bot.name}"),
            FabushiAppAgentSurface.Element("mobile-bot-close", "button", "关闭 Bot 对话"),
            FabushiAppAgentSurface.Element("mobile-bot-settings", "button", "查看 ${bot.name} 设置"),
            FabushiAppAgentSurface.Element("mobile-bot-outline", "button", "查看完整对话"),
            FabushiAppAgentSurface.Element("mobile-bot-draft", "textbox", "Bot 消息"),
        )
        val sendId = if (state.busy) "mobile-bot-stop" else "mobile-bot-send"
        elements += FabushiAppAgentSurface.Element(
            sendId,
            "button",
            if (state.busy) "停止 Bot" else "发送 Bot 消息",
            enabled = state.busy || state.draft.trim().isNotEmpty(),
        )
        state.messages.takeLast(50).forEach { entry ->
            val id = "mobile-bot-entry-${entry.id}".replace(Regex("[^A-Za-z0-9._:/@-]"), "-").take(200)
            val roleName = when {
                entry.role == MobileChatRole.USER -> "用户消息"
                entry.kind == MobileChatEntryKind.ACTION -> "Bot 动作"
                entry.kind == MobileChatEntryKind.THINKING -> "Bot 思考"
                else -> "Bot 消息"
            }
            elements += FabushiAppAgentSurface.Element(id, "log", roleName)
        }
        state.pendingApproval?.let { approval ->
            elements += FabushiAppAgentSurface.Element(
                "mobile-bot-approval",
                "alert",
                "Agent 动作需要批准：${approval.capability}",
            )
            elements += FabushiAppAgentSurface.Element("mobile-bot-approval-approve", "button", "批准 Agent 动作", enabled = !approval.resolving)
            elements += FabushiAppAgentSurface.Element("mobile-bot-approval-deny", "button", "拒绝 Agent 动作", enabled = !approval.resolving)
        }
        if (state.error?.isNotBlank() == true) {
            elements += FabushiAppAgentSurface.Element("mobile-bot-error", "status", "Bot 对话失败")
        }
        val actions = linkedMapOf(
            "mobile-bot-close" to FabushiAppAgentSurface.Action(setOf("invoke")) { onClose() },
            "mobile-bot-settings" to FabushiAppAgentSurface.Action(setOf("invoke")) { onOpenSettings() },
            "mobile-bot-outline" to FabushiAppAgentSurface.Action(setOf("invoke")) { onOpenConversationOutline() },
            "mobile-bot-draft" to FabushiAppAgentSurface.Action(setOf("setValue")) { onDraftChange(it.orEmpty()) },
            sendId to FabushiAppAgentSurface.Action(setOf("invoke")) { if (state.busy) onStop() else onSend() },
        )
        state.pendingApproval?.takeIf { !it.resolving }?.let {
            actions["mobile-bot-approval-approve"] = FabushiAppAgentSurface.Action(setOf("invoke")) { onResolveApproval(true) }
            actions["mobile-bot-approval-deny"] = FabushiAppAgentSurface.Action(setOf("invoke")) { onResolveApproval(false) }
        }
        appAgentSurface.publish(screen = "bot-chat", elements = elements, actions = actions)
    }
    DisposableEffect(appAgentSurface) {
        onDispose { appAgentSurface.clear() }
    }

    Column(Modifier.fillMaxSize().background(GrokMobileBackground).testTag("mobile-bot-chat")) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text("‹", color = GrokMobileInk, fontSize = 34.sp, modifier = Modifier.clickable(onClick = onClose).padding(horizontal = 8.dp))
            Spacer(Modifier.weight(1f))
            Row(
                Modifier
                    .background(Color.White, RoundedCornerShape(28.dp))
                    .clickable(onClick = onOpenSettings)
                    .padding(horizontal = 13.dp, vertical = 7.dp)
                    .testTag("mobile-bot-settings"),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                AgentAvatarAndroid(bot, 28.dp, activeOverride = if (state.busy) true else null)
                Column(Modifier.padding(start = 8.dp)) {
                    Text(bot.name, color = GrokMobileInk, fontSize = 17.sp, fontWeight = FontWeight.SemiBold)
                    if (bot.isRunning && bot.awaitingUserResponse == null) {
                        Text("Working", color = GrokMobileMuted, fontSize = 10.sp)
                    }
                }
            }
            Spacer(Modifier.weight(1f))
            Text(
                "☷",
                color = GrokMobileInk,
                fontSize = 19.sp,
                modifier = Modifier
                    .padding(horizontal = 8.dp)
                    .clickable(onClick = onOpenConversationOutline)
                    .testTag("mobile-bot-outline-open"),
            )
            Text(
                "⌕",
                color = GrokMobileInk,
                fontSize = 20.sp,
                modifier = Modifier
                    .padding(horizontal = 8.dp)
                    .clickable {
                        findOpen = !findOpen
                        if (!findOpen) {
                            findQuery = ""
                            findIndex = -1
                        }
                    }
                    .testTag("mobile-bot-find-open"),
            )
            Text(
                "⌘",
                color = GrokMobileInk,
                fontSize = 20.sp,
                modifier = Modifier
                    .padding(horizontal = 8.dp)
                    .clickable(onClick = onOpenCommandPalette)
                    .testTag("command-palette-open-chat"),
            )
        }
        if (findOpen) {
            FindInChatBar(
                query = findQuery,
                matchCount = findMatches.size,
                currentIndex = findIndex,
                onQueryChange = {
                    findQuery = it
                    findIndex = -1
                },
                onPrevious = {
                    findIndex = stepAgentFindInChatIndex(findIndex, -1, findMatches.size)
                },
                onNext = {
                    findIndex = stepAgentFindInChatIndex(findIndex, 1, findMatches.size)
                },
                onClose = {
                    findOpen = false
                    findQuery = ""
                    findIndex = -1
                },
            )
        }
        LazyColumn(
            state = messageListState,
            modifier = Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            if (state.transcriptLoadError) {
                item(key = "transcript-load-error") {
                    AndroidTranscriptLoadError(onRetry = onRetryTranscriptLoad)
                }
            }
            if (!state.transcriptLoadError && (state.historyHasMore || state.historyLoadingOlder || state.historyError != null)) {
                item(key = "history-loader") {
                    Column(
                        Modifier.fillMaxWidth().padding(vertical = 6.dp),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        when {
                            state.historyLoadingOlder -> Text("Loading earlier messages…", color = GrokMobileMuted, fontSize = 11.sp)
                            state.historyHasMore -> Text(
                                "Load earlier messages",
                                color = Color(0xFF2A92FE),
                                fontSize = 12.sp,
                                fontWeight = FontWeight.SemiBold,
                                modifier = Modifier
                                    .clickable {
                                        val firstVisible = messageListState.layoutInfo.visibleItemsInfo
                                            .firstOrNull { item -> item.key is String && item.key != "history-loader" }
                                        val key = firstVisible?.key as? String
                                        if (key != null) {
                                            historyAnchor = key to firstVisible.offset
                                        }
                                        onLoadOlderMessages()
                                    }
                                    .padding(horizontal = 12.dp, vertical = 6.dp)
                                    .testTag("mobile-bot-load-older"),
                            )
                        }
                        state.historyError?.let { error ->
                            Text(error, color = Color(0xFFD14343), fontSize = 11.sp)
                        }
                    }
                }
            }
            if (!state.transcriptLoadError && state.messages.isEmpty()) item {
                Column(Modifier.fillMaxWidth().padding(top = 96.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    AgentAvatarAndroid(bot, 82.dp)
                    Text(bot.name, color = GrokMobileInk, fontSize = 22.sp, fontWeight = FontWeight.Bold)
                    if (bot.description.isNotBlank()) Text(bot.description, color = GrokMobileMuted, fontSize = 14.sp)
                }
            }
            itemsIndexed(
                items = if (state.transcriptLoadError) emptyList() else state.messages,
                key = { _, entry -> entry.id },
            ) { index, entry ->
                val adjacency = transcriptAdjacency.getOrElse(index) { emptyTranscriptAdjacency() }
                val findHighlight = currentFindMessageId == entry.id
                Column(
                    Modifier
                        .fillMaxWidth()
                        .semantics {
                            contentDescription = when {
                                entry.kind == MobileChatEntryKind.THINKING -> "Agent thinking"
                                entry.kind == MobileChatEntryKind.ACTION -> "Agent action"
                                entry.role == MobileChatRole.USER -> "User message"
                                else -> "Agent message"
                            }
                            if (entry.streaming) stateDescription = "Streaming"
                        }
                        .then(
                            if (findHighlight) {
                                Modifier
                                    .background(Color(0xFFFFF2A8), RoundedCornerShape(10.dp))
                                    .testTag("mobile-bot-find-current")
                            } else {
                                Modifier
                            },
                        ),
                ) {
                when {
                    entry.kind == MobileChatEntryKind.THINKING -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                        AgentAvatarAndroid(bot, 22.dp, activeOverride = true)
                        Text(entry.actionTitle ?: "Thinking…", color = GrokMobileMuted, fontSize = 12.sp)
                        CircularProgressIndicator(Modifier.size(12.dp), strokeWidth = 1.5.dp, color = GrokMobileMuted)
                    }
                    entry.kind == MobileChatEntryKind.ACTION -> Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(7.dp).background(if (entry.actionStatus == "failed") Color.Red else Color(0xFFFF7A1A), CircleShape))
                        Text(entry.actionTitle ?: "Working", color = GrokMobileInk, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                        entry.actionDetail?.takeIf { it.isNotBlank() }?.let { Text(it, color = GrokMobileMuted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                    }
                    entry.kind == MobileChatEntryKind.TOOL_RESULT && entry.toolResult != null ->
                        MobileToolResultTranscriptCard(entry.toolResult)
                    entry.kind == MobileChatEntryKind.WIDGET && entry.widget != null ->
                        MobileWidgetTranscriptCard(
                            entry = entry,
                            pending = entry.id in state.widgetPendingEntryIds,
                            onRespond = { value -> onRespondToWidget(entry.id, value) },
                            onDismiss = { onDismissWidget(entry.id) },
                        )
                    entry.role == MobileChatRole.USER -> Column(
                        Modifier.fillMaxWidth(),
                        horizontalAlignment = Alignment.End,
                        verticalArrangement = Arrangement.spacedBy(5.dp),
                    ) {
                        if (entry.text.isNotBlank() || !entry.richTextJson.isNullOrBlank()) {
                            MobileUserRichTranscriptContent(
                                text = entry.text,
                                richTextJson = entry.richTextJson,
                                bubbleShape = mobileTranscriptBubbleShape(
                                    role = MobileChatRole.USER,
                                    adjacency = adjacency,
                                ),
                            )
                        }
                        MobileTranscriptAttachments(entry.attachments, alignEnd = true)
                        when (entry.sendStatus) {
                            "pending" -> Text("等待入队…", color = GrokMobileMuted, fontSize = 11.sp)
                            "queued" -> Row(
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text("已入队…", color = GrokMobileMuted, fontSize = 11.sp)
                                entry.clientNonce?.takeIf(String::isNotBlank)?.let { nonce ->
                                    Text(
                                        "取消",
                                        color = Color(0xFF2A92FE),
                                        fontSize = 11.sp,
                                        fontWeight = FontWeight.SemiBold,
                                        modifier = Modifier
                                            .clickable { onCancelQueuedSend(nonce) }
                                            .padding(vertical = 4.dp)
                                            .testTag("mobile-bot-cancel-queued-send"),
                                    )
                                }
                            }
                            "dispatching" -> Text("发送中…", color = GrokMobileMuted, fontSize = 11.sp)
                            "accepted-awaiting-echo" -> Text("已接受，等待同步…", color = GrokMobileMuted, fontSize = 11.sp)
                            "outcome-unknown" -> Text(
                                "发送结果待确认；不会自动重试",
                                color = Color(0xFFE18900),
                                fontSize = 11.sp,
                            )
                            "failed" -> Row(
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text("发送失败", color = Color.Red, fontSize = 11.sp)
                                entry.clientNonce?.takeIf(String::isNotBlank)?.let { nonce ->
                                    Text(
                                        "重试",
                                        color = Color(0xFF2A92FE),
                                        fontSize = 11.sp,
                                        fontWeight = FontWeight.SemiBold,
                                        modifier = Modifier
                                            .clickable { onRetrySend(nonce) }
                                            .padding(vertical = 4.dp),
                                    )
                                }
                            }
                        }
                    }
                    else -> Column {
                        if (adjacency.isRunStart || adjacency.isGroupStart) {
                            Text(
                                entry.transcriptAuthor?.takeIf(String::isNotBlank) ?: bot.name,
                                color = GrokMobileMuted,
                                fontSize = 11.sp,
                                modifier = Modifier.padding(start = 29.dp, bottom = 2.dp),
                            )
                        }
                        Row(verticalAlignment = Alignment.Bottom) {
                            if (adjacency.isContinuedFromPrev) {
                                Spacer(Modifier.size(20.dp))
                            } else {
                                AgentAvatarAndroid(bot, 20.dp)
                            }
                            Column(
                                Modifier.padding(start = 7.dp),
                                verticalArrangement = Arrangement.spacedBy(5.dp),
                            ) {
                                if (entry.urlCardUrl != null) {
                                    MobileTranscriptUrlCard(entry.urlCardUrl)
                                } else {
                                    if (entry.text.isNotBlank() || entry.streaming) {
                                        MobileAssistantRichTranscriptContent(
                                            text = entry.text,
                                            streaming = entry.streaming,
                                            bubbleShape = mobileTranscriptBubbleShape(
                                                role = MobileChatRole.ASSISTANT,
                                                adjacency = adjacency,
                                            ),
                                        )
                                    }
                                    MobileAssistantTranscriptImages(
                                        images = entry.assistantImages,
                                        sourceTrusted = entry.assistantSourceTrusted,
                                    )
                                    MobileAssistantTranscriptChannel(entry.assistantChannel)
                                }
                                MobileTranscriptAttachments(entry.attachments, alignEnd = false)
                            }
                        }
                    }
                }
                }
            }
            state.pendingApproval?.takeUnless { state.transcriptLoadError }?.let { approval ->
                item(key = "approval:${approval.approvalId}") {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .background(Color(0xFFFFF3E8), RoundedCornerShape(14.dp))
                            .padding(12.dp)
                            .testTag("mobile-bot-approval"),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text("Agent action requires approval", color = GrokMobileInk, fontSize = 14.sp, fontWeight = FontWeight.SemiBold)
                        Text(approval.reason, color = GrokMobileMuted, fontSize = 12.sp)
                        Text(approval.capability, color = GrokMobileMuted, fontSize = 11.sp)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Button(onClick = { onResolveApproval(false) }, enabled = !approval.resolving, modifier = Modifier.testTag("mobile-bot-approval-deny")) { Text("Deny") }
                            Button(onClick = { onResolveApproval(true) }, enabled = !approval.resolving, modifier = Modifier.testTag("mobile-bot-approval-approve")) { Text(if (approval.resolving) "Resolving…" else "Approve") }
                        }
                    }
                }
            }
            state.error?.takeUnless { state.transcriptLoadError }?.let { error -> item { Text(error, color = Color(0xFFD14343), fontSize = 12.sp) } }
        }
        Row(Modifier.fillMaxWidth().padding(10.dp), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f)) {
                OutlinedTextField(
                    state.draft,
                    onDraftChange,
                    placeholder = { Text("Message") },
                    maxLines = 5,
                    modifier = Modifier.fillMaxWidth().testTag("mobile-bot-draft"),
                    shape = RoundedCornerShape(19.dp),
                )
                if (bot.miniAppId == null) {
                    if (state.draftAttachments.isNotEmpty()) {
                        Column(
                            Modifier
                                .fillMaxWidth()
                                .padding(top = 6.dp),
                            verticalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            state.draftAttachments.forEach { attachment ->
                                Row(
                                    Modifier
                                        .fillMaxWidth()
                                        .background(Color.Black.copy(alpha = 0.045f), RoundedCornerShape(10.dp))
                                        .padding(horizontal = 10.dp, vertical = 6.dp)
                                        .testTag("mobile-bot-draft-attachment-" + attachment.id),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        Text(
                                            attachment.name,
                                            color = GrokMobileInk,
                                            fontSize = 12.sp,
                                            fontWeight = FontWeight.SemiBold,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                        Text(
                                            listOfNotNull(
                                                attachment.mimeType,
                                                formatTranscriptAttachmentBytes(attachment.sizeBytes),
                                            ).joinToString(" · "),
                                            color = GrokMobileMuted,
                                            fontSize = 10.sp,
                                        )
                                    }
                                    TextButton(
                                        onClick = { onRemoveDraftAttachment(attachment.id) },
                                        enabled = !state.busy && !state.attachmentBusy,
                                        modifier = Modifier.testTag("mobile-bot-draft-attachment-remove-" + attachment.id),
                                    ) { Text("Remove") }
                                }
                            }
                        }
                    }
                    TextButton(
                        onClick = { attachmentPicker.launch(arrayOf("*/*")) },
                        enabled = !state.busy && !state.attachmentBusy && state.draftAttachments.size < 6,
                        modifier = Modifier.testTag("mobile-bot-attach"),
                    ) {
                        if (state.attachmentBusy) {
                            CircularProgressIndicator(Modifier.size(12.dp), strokeWidth = 1.5.dp)
                            Spacer(Modifier.size(6.dp))
                        }
                        Text(if (state.attachmentBusy) "Attaching…" else "Attach")
                    }
                }
                val editorSuggestions = state.editorSuggestions
                if (editorSuggestions.rows.isNotEmpty() && editorSuggestions.status in setOf(MobileEditorSuggestionStatus.LOADING, MobileEditorSuggestionStatus.READY, MobileEditorSuggestionStatus.FAILED)) {
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .padding(top = 4.dp)
                            .background(Color(0xFFF6F6F7), RoundedCornerShape(12.dp))
                            .padding(vertical = 4.dp)
                            .testTag("mobile-bot-editor-suggestions"),
                    ) {
                        editorSuggestions.rows.take(8).forEach { suggestion ->
                            Row(
                                Modifier
                                    .fillMaxWidth()
                                    .clickable {
                                        onDraftChange(
                                            applyMobileEditorSuggestion(
                                                state.draft,
                                                editorSuggestions,
                                                suggestion,
                                            ),
                                        )
                                    }
                                    .padding(horizontal = 12.dp, vertical = 8.dp)
                                    .testTag("mobile-bot-editor-suggestion-" + suggestion.key.replace(Regex("[^A-Za-z0-9._-]"), "-")),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    when (suggestion.category) {
                                        MobileEditorSuggestionCategory.ASSISTANT -> if (suggestion.isGroup) "◎" else "@"
                                        MobileEditorSuggestionCategory.AUTOMATION -> "◷"
                                        MobileEditorSuggestionCategory.WORKFLOW -> "/"
                                    },
                                    color = GrokMobileMuted,
                                    fontSize = 13.sp,
                                    modifier = Modifier.padding(end = 8.dp),
                                )
                                Column(Modifier.weight(1f)) {
                                    Text(suggestion.label, color = GrokMobileInk, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                                    suggestion.subtitle?.let { subtitle ->
                                        Text(subtitle, color = GrokMobileMuted, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if (!state.busy) {
                OfflineAsrDraftControl(
                    currentDraft = state.draft,
                    enabled = true,
                    onDraftChange = onDraftChange,
                    sessionKey = bot.id,
                    testTag = "mobile-bot-offline-asr",
                )
            }
            Button(
                onClick = if (state.busy) onStop else onSend,
                enabled = state.busy || (
                    !state.attachmentBusy &&
                        (state.draft.trim().isNotEmpty() || state.draftAttachments.isNotEmpty())
                    ),
                modifier = Modifier.size(48.dp).testTag(if (state.busy) "mobile-bot-stop" else "mobile-bot-send"),
                colors = ButtonDefaults.buttonColors(containerColor = if (state.busy) Color(0xFFE34B5F) else Color.Black, contentColor = Color.White),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(0.dp),
            ) { Text(if (state.busy) "■" else "↑", fontSize = 19.sp, fontWeight = FontWeight.Bold) }
        }
    }
}


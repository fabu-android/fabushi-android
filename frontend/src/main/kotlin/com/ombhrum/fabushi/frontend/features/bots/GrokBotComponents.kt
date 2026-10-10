package com.ombhrum.fabushi

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
        ClothGhostAvatarAndroid(
            botId = bot.id,
            size = 47.dp,
            badge = Color(0xFF20B967),
            avatarShape = bot.avatarShape,
            avatarColor = bot.avatarColor,
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
    onDraftChange: (String) -> Unit,
    onSend: () -> Unit,
    onStop: () -> Unit,
    onResolveApproval: (Boolean) -> Unit,
    onMessageTargetConsumed: (String) -> Unit,
) {
    val messageListState = rememberLazyListState()
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
            Row(Modifier.background(Color.White, RoundedCornerShape(28.dp)).padding(horizontal = 13.dp, vertical = 7.dp), verticalAlignment = Alignment.CenterVertically) {
                ClothGhostAvatarAndroid(bot.id, 28.dp, active = state.busy)
                Text(bot.name, color = GrokMobileInk, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(start = 8.dp))
            }
            Spacer(Modifier.weight(1f))
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
        LazyColumn(
            state = messageListState,
            modifier = Modifier.weight(1f).fillMaxWidth().padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            if (state.messages.isEmpty()) item {
                Column(Modifier.fillMaxWidth().padding(top = 96.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    ClothGhostAvatarAndroid(bot.id, 82.dp)
                    Text(bot.name, color = GrokMobileInk, fontSize = 22.sp, fontWeight = FontWeight.Bold)
                    if (bot.description.isNotBlank()) Text(bot.description, color = GrokMobileMuted, fontSize = 14.sp)
                }
            }
            items(state.messages, key = { it.id }) { entry ->
                when {
                    entry.kind == MobileChatEntryKind.THINKING -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                        ClothGhostAvatarAndroid(bot.id, 22.dp, active = true)
                        Text(entry.actionTitle ?: "Thinking…", color = GrokMobileMuted, fontSize = 12.sp)
                        CircularProgressIndicator(Modifier.size(12.dp), strokeWidth = 1.5.dp, color = GrokMobileMuted)
                    }
                    entry.kind == MobileChatEntryKind.ACTION -> Row(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(Modifier.size(7.dp).background(if (entry.actionStatus == "failed") Color.Red else Color(0xFFFF7A1A), CircleShape))
                        Text(entry.actionTitle ?: "Working", color = GrokMobileInk, fontSize = 12.sp, fontWeight = FontWeight.Medium)
                        entry.actionDetail?.takeIf { it.isNotBlank() }?.let { Text(it, color = GrokMobileMuted, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                    }
                    entry.role == MobileChatRole.USER -> Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                        Text(entry.text, color = Color.White, fontSize = 16.sp, modifier = Modifier.background(Color.Black, RoundedCornerShape(18.dp)).padding(horizontal = 15.dp, vertical = 10.dp))
                    }
                    else -> Column {
                        Text(bot.name, color = GrokMobileMuted, fontSize = 11.sp, modifier = Modifier.padding(start = 29.dp, bottom = 2.dp))
                        Row(verticalAlignment = Alignment.Bottom) {
                            ClothGhostAvatarAndroid(bot.id, 20.dp)
                            Text(entry.text + if (entry.streaming) "▌" else "", color = GrokMobileInk, fontSize = 16.sp, modifier = Modifier.padding(start = 7.dp).background(Color.Black.copy(alpha = 0.055f), RoundedCornerShape(18.dp)).padding(horizontal = 15.dp, vertical = 10.dp))
                        }
                    }
                }
            }
            state.pendingApproval?.let { approval ->
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
            state.error?.let { error -> item { Text(error, color = Color(0xFFD14343), fontSize = 12.sp) } }
        }
        Row(Modifier.fillMaxWidth().padding(10.dp), verticalAlignment = Alignment.Bottom, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                state.draft,
                onDraftChange,
                placeholder = { Text("Message") },
                maxLines = 5,
                modifier = Modifier.weight(1f).testTag("mobile-bot-draft"),
                shape = RoundedCornerShape(19.dp),
            )
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
                enabled = state.busy || state.draft.trim().isNotEmpty(),
                modifier = Modifier.size(48.dp).testTag(if (state.busy) "mobile-bot-stop" else "mobile-bot-send"),
                colors = ButtonDefaults.buttonColors(containerColor = if (state.busy) Color(0xFFE34B5F) else Color.Black, contentColor = Color.White),
                contentPadding = androidx.compose.foundation.layout.PaddingValues(0.dp),
            ) { Text(if (state.busy) "■" else "↑", fontSize = 19.sp, fontWeight = FontWeight.Bold) }
        }
    }
}


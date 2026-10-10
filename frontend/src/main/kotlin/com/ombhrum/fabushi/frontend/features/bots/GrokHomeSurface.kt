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

internal val GrokMobileBackground = Color(0xFFFAFAF7)
internal val GrokMobileInk = Color(0xFF111111)
internal val GrokMobileMuted = Color(0xFF8B8B8B)

@Composable
fun GrokHomeSurface(
    accountName: String,
    messagingState: MessagingUiState,
    botState: MobileBotUiState,
    appAgentSurface: FabushiAppAgentSurface,
    onOpenMessaging: () -> Unit,
    onOpenAgentNetwork: () -> Unit,
    onOpenCommandPalette: () -> Unit,
    onRefreshBots: () -> Unit,
    onCreateBot: (String, String, (() -> Unit)?) -> Unit,
    onOpenBot: (MobileBotSummaryAndroid) -> Unit,
    onRenameBot: (String, String) -> Unit,
    onHideBot: (String) -> Unit,
    onSetBotUnread: (String, Boolean) -> Unit,
    onDuplicateBot: (String) -> Unit,
    onDeleteBot: suspend (String) -> Unit,
    onSetBotPinned: (String, Boolean) -> Unit,
    onCloseBot: () -> Unit,
    onDraftChange: (String) -> Unit,
    onSend: () -> Unit,
    onStop: () -> Unit,
    onMessageTargetConsumed: (String) -> Unit,
) {
    LaunchedEffect(Unit) { onRefreshBots() }
    val active = botState.activeBot
    if (active != null) {
        GrokBotChatAndroid(
            active,
            botState,
            appAgentSurface,
            onCloseBot,
            onOpenCommandPalette,
            onDraftChange,
            onSend,
            onStop,
            onMessageTargetConsumed,
        )
        return
    }

    var query by remember { mutableStateOf("") }
    var addOpen by remember { mutableStateOf(false) }
    var createOpen by remember { mutableStateOf(false) }
    var botName by remember { mutableStateOf("") }
    var botDescription by remember { mutableStateOf("") }
    var editingBotId by remember { mutableStateOf<String?>(null) }
    var deleteTarget by remember { mutableStateOf<AgentDeleteTarget?>(null) }
    var showHiddenBots by remember { mutableStateOf(false) }

    LaunchedEffect(
        query,
        addOpen,
        createOpen,
        botName,
        botDescription,
        botState.creating,
        botState.error,
        botState.bots,
        botState.rosterLoading,
        showHiddenBots,
        messagingState.conversations,
        appAgentSurface,
    ) {
        val elements = mutableListOf<FabushiAppAgentSurface.Element>()
        val actions = linkedMapOf<String, FabushiAppAgentSurface.Action>()
        fun element(
            id: String,
            role: String,
            name: String,
            enabled: Boolean = true,
            action: FabushiAppAgentSurface.Action? = null,
        ) {
            val agentId = id.replace(Regex("[^A-Za-z0-9._:/@-]"), "-").take(200)
            elements += FabushiAppAgentSurface.Element(
                agentId = agentId,
                role = role.take(80),
                name = name.take(240),
                enabled = enabled,
            )
            if (action != null) actions[agentId] = action
        }

        val screen = when {
            createOpen -> {
                element("grok-create-bot", "dialog", "创建 Bot")
                element(
                    "new-bot-name",
                    "textbox",
                    "Bot 名称",
                    action = FabushiAppAgentSurface.Action(setOf("setValue")) { botName = it.orEmpty() },
                )
                element(
                    "new-bot-description",
                    "textbox",
                    "Bot 描述",
                    action = FabushiAppAgentSurface.Action(setOf("setValue")) { botDescription = it.orEmpty() },
                )
                element(
                    "create-bot-submit",
                    "button",
                    "创建 Bot",
                    enabled = botName.isNotBlank() && !botState.creating,
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) {
                        if (botName.isNotBlank() && !botState.creating) {
                            onCreateBot(botName, botDescription) {
                                botName = ""
                                botDescription = ""
                                createOpen = false
                            }
                        }
                    },
                )
                element(
                    "create-bot-cancel",
                    "button",
                    "取消创建 Bot",
                    enabled = !botState.creating,
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) { if (!botState.creating) createOpen = false },
                )
                botState.error?.takeIf { it.isNotBlank() }?.let { element("create-bot-error", "status", "Bot 创建失败") }
                "grok-create-bot"
            }
            else -> {
                element("grok-mobile-home", "application", "Fabushi")
                element(
                    "grok-mobile-messages",
                    "button",
                    "打开消息与功能",
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) { onOpenMessaging() },
                )
                element(
                    "grok-mobile-agent-network",
                    "button",
                    "Agent network",
                    enabled = botState.bots.isNotEmpty(),
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) {
                        if (botState.bots.isNotEmpty()) onOpenAgentNetwork()
                    },
                )
                element(
                    "grok-mobile-search-toggle",
                    "button",
                    if (query.isEmpty()) "打开搜索" else "关闭搜索",
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) { query = if (query.isEmpty()) " " else "" },
                )
                if (query.isNotEmpty()) {
                    element(
                        "grok-mobile-search-field",
                        "textbox",
                        "搜索",
                        action = FabushiAppAgentSurface.Action(setOf("setValue")) { query = it.orEmpty() },
                    )
                }
                element(
                    "grok-mobile-add",
                    "button",
                    "新建",
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) { addOpen = true },
                )
                if (addOpen) {
                    element(
                        "grok-mobile-new-bot",
                        "menuitem",
                        "New Bot",
                        action = FabushiAppAgentSurface.Action(setOf("invoke")) { addOpen = false; createOpen = true },
                    )
                    for ((id, name) in listOf(
                        "grok-mobile-new-message" to "New message",
                        "grok-mobile-new-group" to "New group",
                        "grok-mobile-new-channel" to "New channel",
                    )) {
                        element(
                            id,
                            "menuitem",
                            name,
                            action = FabushiAppAgentSurface.Action(setOf("invoke")) { addOpen = false; onOpenMessaging() },
                        )
                    }
                }
                val mahayana = MobileBotSummaryAndroid("mahayana-assistant", "Mahayana", "that's the only new one.")
                element(
                    "grok-bot-mahayana-assistant",
                    "button",
                    "打开 Mahayana",
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) { onOpenBot(mahayana) },
                )
                botState.bots
                    .filter { showHiddenBots || !it.isHidden }
                    .filter { query.isBlank() || it.name.contains(query.trim(), true) || it.description.contains(query.trim(), true) }
                    .take(100)
                    .forEach { bot ->
                        element(
                            "grok-bot-${bot.id}",
                            "button",
                            "打开 ${bot.name}",
                            action = FabushiAppAgentSurface.Action(setOf("invoke")) { onOpenBot(bot) },
                        )
                    }
                botState.error?.takeIf { it.isNotBlank() }?.let { diagnostic ->
                    element("grok-bot-error", "status", "Bot 刷新异常：$diagnostic")
                }
                if (addOpen) "grok-compose" else "grok-home"
            }
        }
        appAgentSurface.publish(screen = screen, elements = elements, actions = actions)
    }
    DisposableEffect(appAgentSurface) {
        onDispose { appAgentSurface.clear() }
    }

    if (createOpen) {
        AlertDialog(
            onDismissRequest = { if (!botState.creating) createOpen = false },
            title = { Text("New Bot") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    ClothGhostAvatarAndroid(botName.ifBlank { "new-bot" }, 82.dp, active = botState.creating)
                    OutlinedTextField(botName, { botName = it }, label = { Text("Bot name") }, modifier = Modifier.testTag("new-bot-name"), singleLine = true)
                    OutlinedTextField(botDescription, { botDescription = it }, label = { Text("What does this Bot do?") }, minLines = 2, maxLines = 4)
                    botState.error?.let { Text(it, color = Color(0xFFD14343), fontSize = 12.sp) }
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        onCreateBot(botName, botDescription) {
                            botName = ""
                            botDescription = ""
                            createOpen = false
                        }
                    },
                    enabled = botName.isNotBlank() && !botState.creating,
                    modifier = Modifier.testTag("create-bot-submit"),
                ) { Text(if (botState.creating) "Creating…" else "Create") }
            },
            dismissButton = { TextButton(onClick = { createOpen = false }, enabled = !botState.creating) { Text("Cancel") } },
        )
    }

    Box(Modifier.fillMaxSize().background(GrokMobileBackground).testTag("grok-mobile-home")) {
        LazyColumn(Modifier.fillMaxSize()) {
            item {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(
                        Modifier.size(38.dp).background(Color(0xFFFFC7D1), CircleShape).clickable(onClick = onOpenMessaging),
                        contentAlignment = Alignment.Center,
                    ) { Text(accountName.take(1).uppercase().ifBlank { "F" }, color = GrokMobileInk, fontWeight = FontWeight.Bold) }
                    Spacer(Modifier.weight(1f))
                    Text(
                        "◇",
                        fontSize = 22.sp,
                        color = if (botState.bots.isNotEmpty()) GrokMobileInk else GrokMobileMuted.copy(alpha = 0.35f),
                        modifier = Modifier
                            .padding(horizontal = 8.dp)
                            .clickable(enabled = botState.bots.isNotEmpty(), onClick = onOpenAgentNetwork)
                            .testTag("agent-network-open"),
                    )
                    Text("⌕", fontSize = 29.sp, color = GrokMobileInk, modifier = Modifier.padding(horizontal = 10.dp).clickable { query = if (query.isEmpty()) " " else "" })
                    Text(
                        "⌘",
                        fontSize = 20.sp,
                        color = GrokMobileInk,
                        modifier = Modifier
                            .padding(horizontal = 8.dp)
                            .clickable(onClick = onOpenCommandPalette)
                            .testTag("command-palette-open-home"),
                    )
                    Box {
                        Text("+", fontSize = 31.sp, color = GrokMobileInk, modifier = Modifier.padding(horizontal = 8.dp).clickable { addOpen = true }.testTag("grok-mobile-add"))
                        DropdownMenu(expanded = addOpen, onDismissRequest = { addOpen = false }) {
                            DropdownMenuItem(text = { Text("New Bot") }, onClick = { addOpen = false; createOpen = true })
                            DropdownMenuItem(text = { Text("New message") }, onClick = { addOpen = false; onOpenMessaging() })
                            DropdownMenuItem(text = { Text("New group") }, onClick = { addOpen = false; onOpenMessaging() })
                            DropdownMenuItem(text = { Text("New channel") }, onClick = { addOpen = false; onOpenMessaging() })
                        }
                    }
                }
            }
            item {
                Column(
                    Modifier.fillMaxWidth().padding(top = 30.dp, bottom = 34.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Box(Modifier.size(width = 140.dp, height = 88.dp), contentAlignment = Alignment.Center) {
                        ClothGhostAvatarAndroid("all-hands-green", 52.dp, modifier = Modifier.align(Alignment.Center).padding(end = 58.dp, top = 13.dp))
                        ClothGhostAvatarAndroid("all-hands-violet", 52.dp, modifier = Modifier.align(Alignment.Center).padding(start = 2.dp, top = 26.dp))
                        ClothGhostAvatarAndroid("mahayana-assistant", 57.dp, modifier = Modifier.align(Alignment.Center).padding(start = 56.dp))
                        Text("+2", color = GrokMobileInk.copy(alpha = 0.35f), fontSize = 28.sp, fontWeight = FontWeight.Bold, modifier = Modifier.align(Alignment.BottomEnd))
                    }
                    Text("All Hands", color = GrokMobileMuted, fontSize = 14.sp)
                }
            }
            if (query.isNotEmpty()) item {
                OutlinedTextField(
                    value = query.trimStart(),
                    onValueChange = { query = it },
                    placeholder = { Text("Search") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
                    shape = RoundedCornerShape(15.dp),
                )
            }
            item { SectionLabelAndroid("Board") }
            item {
                GrokBotRowAndroid(
                    MobileBotSummaryAndroid("mahayana-assistant", "Mahayana", "that's the only new one."),
                    badge = "Board",
                    onClick = onOpenBot,
                )
            }
            val matchingBots = botState.bots.filter {
                query.isBlank() ||
                    it.name.contains(query.trim(), true) ||
                    it.description.contains(query.trim(), true)
            }
            val visibleBots = matchingBots.filter { showHiddenBots || !it.isHidden }
            val allMatchingBotsHidden =
                matchingBots.isNotEmpty() &&
                    visibleBots.isEmpty() &&
                    matchingBots.all { it.isHidden }

            when {
                botState.rosterLoading && botState.bots.isEmpty() -> {
                    item {
                        RosterStatus(
                            kind = RosterStatusKind.LOADING,
                            isRetrying = true,
                            modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp),
                        )
                    }
                }
                botState.error != null && botState.bots.isEmpty() -> {
                    item {
                        RosterStatus(
                            kind = RosterStatusKind.ERROR,
                            isRetrying = botState.rosterLoading,
                            onRetry = onRefreshBots,
                            modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp),
                        )
                    }
                }
                allMatchingBotsHidden -> {
                    item {
                        RosterStatus(
                            kind = RosterStatusKind.ALL_HIDDEN,
                            onShowHiddenBots = { showHiddenBots = true },
                            modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp),
                        )
                    }
                }
                botState.bots.isEmpty() -> {
                    item {
                        RosterStatus(
                            kind = RosterStatusKind.EMPTY,
                            modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp),
                        )
                    }
                }
            }

            if (visibleBots.isNotEmpty()) {
                item { SectionLabelAndroid("Bots  ${visibleBots.size}") }
                items(visibleBots, key = { it.id }) { bot ->
                    GrokBotRowAndroid(
                        bot = bot,
                        badge = "Bot",
                        onClick = onOpenBot,
                        editingName = editingBotId == bot.id,
                        onNameCommit = { nextName ->
                            onRenameBot(bot.id, nextName)
                        },
                        onNameExit = {
                            editingBotId = null
                        },
                        trailing = {
                            AgentRowActions(
                                agentId = bot.id,
                                agentName = bot.name,
                                isGroup = bot.isGroup,
                                isPinned = bot.isPinned,
                                hasUnread = bot.hasUnread,
                                isHidden = bot.isHidden,
                                onEditName = { editingBotId = it },
                                onHideFromSidebar = onHideBot,
                                onDuplicateAgent = onDuplicateBot,
                                onTogglePin = onSetBotPinned,
                                onSetAgentUnread = onSetBotUnread,
                                onRequestDelete = { deleteTarget = it },
                            )
                        },
                    )
                }
            }
            botState.error?.takeIf { it.isNotBlank() }?.let {
                if (botState.bots.isNotEmpty()) {
                    item {
                        RosterReconnectNotice(
                            isRetrying = botState.rosterLoading,
                            onRetry = onRefreshBots,
                            modifier = Modifier.padding(horizontal = 18.dp, vertical = 8.dp),
                        )
                    }
                }
            }
            val rows = messagingState.conversations.filter { !it.isArchived && (query.isBlank() || it.title.contains(query.trim(), true) || it.preview.contains(query.trim(), true)) }
            if (rows.isNotEmpty()) {
                item { SectionLabelAndroid("Projects  ${rows.size}") }
                items(rows.take(10), key = { it.id }) { conversation ->
                    Row(
                        Modifier.fillMaxWidth().clickable(onClick = onOpenMessaging).padding(horizontal = 18.dp, vertical = 9.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        ClothGhostAvatarAndroid("conversation:${conversation.id}", 45.dp, badge = if (conversation.unreadCount > 0) Color(0xFF2A92FE) else null)
                        Column(Modifier.weight(1f).padding(start = 12.dp)) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(conversation.title, color = GrokMobileInk, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                                Text(if (conversation.kind == ConversationKind.CHANNEL) "Channel" else "Engineering", color = GrokMobileMuted, fontSize = 11.sp, modifier = Modifier.padding(start = 7.dp).background(Color.Black.copy(alpha = 0.045f), RoundedCornerShape(20.dp)).padding(horizontal = 7.dp, vertical = 3.dp))
                            }
                            Text(conversation.preview.ifBlank { "Ready" }, color = GrokMobileMuted, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                        Text(conversation.time, color = GrokMobileMuted, fontSize = 11.sp, modifier = Modifier.padding(start = 6.dp))
                    }
                }
            }
            item { Spacer(Modifier.height(44.dp)) }
        }

        AgentDeleteConfirmation(
            agent = deleteTarget,
            onClose = { deleteTarget = null },
            onConfirm = onDeleteBot,
        )
    }
}


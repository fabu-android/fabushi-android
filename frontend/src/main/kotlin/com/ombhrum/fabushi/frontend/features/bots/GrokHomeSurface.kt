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
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessBlockReason
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessProjection
import com.ombhrum.fabushi.androidpreload.runtime.AccountAccessState
import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointAvailability
import com.ombhrum.fabushi.androidpreload.runtime.AndroidSidebarSection
import kotlin.math.absoluteValue
import kotlin.math.sin

internal val GrokMobileBackground = Color(0xFFFAFAF7)
internal val GrokMobileInk = Color(0xFF111111)
internal val GrokMobileMuted = Color(0xFF8B8B8B)

@Composable
internal fun GrokHomeSurface(
    accountName: String,
    accessProjection: AccountAccessProjection,
    rosterAccessReadiness: RosterAccessReadiness,
    messagingState: MessagingUiState,
    botState: MobileBotUiState,
    appAgentSurface: FabushiAppAgentSurface,
    agentNetworkAvailability: AndroidEntrypointAvailability,
    hiddenChatsAvailability: AndroidEntrypointAvailability,
    onOpenMessaging: () -> Unit,
    onRefreshAccess: () -> Unit,
    onOpenAccessOnboarding: () -> Unit,
    onOpenAgentNetwork: () -> Unit,
    onOpenCommandPalette: () -> Unit,
    onOpenConversationOutline: (MobileBotSummaryAndroid) -> Unit,
    onCloseConversationOutline: () -> Unit,
    onSelectConversationOutlineAgent: (String) -> Unit,
    onRefreshConversationOutline: () -> Unit,
    onShowBotAsyncTasks: (MobileBotSummaryAndroid) -> Unit,
    onRefreshBots: () -> Unit,
    onCreateBot: (String, String, (() -> Unit)?) -> Unit,
    onCreateGroup: (String, String, List<String>, (() -> Unit)?) -> Unit,
    onSetGroupMembers: (String, List<String>, (() -> Unit)?) -> Unit,
    onOpenBot: (MobileBotSummaryAndroid) -> Unit,
    onRenameBot: (String, String) -> Unit,
    onBeginAgentSettings: (String) -> Unit,
    onEndAgentSettings: (String) -> Unit,
    onUpdateBotProfile: (String, String, String?, String, String?, String?) -> Unit,
    onSetBotNotifyOnUpdates: (String, Boolean) -> Unit,
    onHideBot: (String) -> Unit,
    onUnhideBot: (String) -> Unit,
    onSetBotUnread: (String, Boolean) -> Unit,
    onDuplicateBot: (String) -> Unit,
    onDeleteBot: suspend (String) -> Unit,
    onSetBotPinned: (String, Boolean) -> Unit,
    onReorderPinnedBot: (String, String, PinnedMovePosition) -> Unit,
    onToggleSidebarSection: (String) -> Unit,
    onRenameSidebarSection: (String, String) -> Unit,
    onDeleteSidebarSection: (String) -> Unit,
    onMoveSidebarSection: (String, String, AgentSidebarSectionMovePosition) -> Unit,
    onMoveBotToSection: (String, String) -> Unit,
    onMoveBotToNewSection: (String) -> Unit,
    onCloseBot: () -> Unit,
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
    shortcutNewAgentNonce: Int = 0,
    shortcutFocusPromptNonce: Int = 0,
    shortcutFindInChatNonce: Int = 0,
    onPromptEditableFocusChanged: (Boolean) -> Unit = {},
) {
    LaunchedEffect(Unit) { onRefreshBots() }
    var profileTarget by remember { mutableStateOf<MobileBotSummaryAndroid?>(null) }
    val active = botState.activeBot
    if (active != null) {
        val profileTargetId = profileTarget?.id
        DisposableEffect(profileTargetId) {
            onDispose {
                profileTargetId?.let(onEndAgentSettings)
            }
        }
        LaunchedEffect(profileTargetId, botState.bots) {
            val currentId = profileTargetId ?: return@LaunchedEffect
            val authoritative = botState.bots.firstOrNull { it.id == currentId }
            if (authoritative == null) {
                profileTarget = null
            } else if (authoritative != profileTarget) {
                profileTarget = authoritative
            }
        }
        val canonicalRosterAgent = botState.bots.any { it.id == active.id }
        GrokBotChatAndroid(
            bot = active,
            state = botState,
            interactionReady = !canonicalRosterAgent || rosterAccessReadiness.isSelectionReady,
            appAgentSurface = appAgentSurface,
            onClose = onCloseBot,
            onOpenCommandPalette = onOpenCommandPalette,
            onOpenConversationOutline = { onOpenConversationOutline(active) },
            onOpenSettings = {
                onBeginAgentSettings(active.id)
                profileTarget = active
            },
            onDraftChange = onDraftChange,
            onAddDraftAttachments = onAddDraftAttachments,
            onRemoveDraftAttachment = onRemoveDraftAttachment,
            onSend = onSend,
            onRetrySend = onRetrySend,
            onCancelQueuedSend = onCancelQueuedSend,
            onRetryTranscriptLoad = onRetryTranscriptLoad,
            onLoadOlderMessages = onLoadOlderMessages,
            onRespondToWidget = onRespondToWidget,
            onDismissWidget = onDismissWidget,
            onStop = onStop,
            onResolveApproval = onResolveApproval,
            onMessageTargetConsumed = onMessageTargetConsumed,
            shortcutFocusPromptNonce = shortcutFocusPromptNonce,
            shortcutFindInChatNonce = shortcutFindInChatNonce,
            onPromptEditableFocusChanged = onPromptEditableFocusChanged,
        )
        if (botState.conversationOutline.isOpen) {
            ConversationOutlinePanelAndroid(
                state = botState.conversationOutline,
                onSelectAgent = onSelectConversationOutlineAgent,
                onRefresh = onRefreshConversationOutline,
                onClose = onCloseConversationOutline,
            )
        }
        AgentProfileEditor(
            agent = profileTarget,
            onClose = { profileTarget = null },
            onConfirm = { id, name, title, description, avatarShape, avatarColor ->
                onUpdateBotProfile(id, name, title, description, avatarShape, avatarColor)
                profileTarget = null
            },
            onSetNotifications = onSetBotNotifyOnUpdates,
        )
        return
    }

    var query by remember { mutableStateOf("") }
    var addOpen by remember { mutableStateOf(false) }
    var createOpen by remember { mutableStateOf(false) }
    var createGroupOpen by remember { mutableStateOf(false) }
    var groupName by remember { mutableStateOf("") }
    var groupDescription by remember { mutableStateOf("") }
    var groupMemberIds by remember { mutableStateOf(setOf<String>()) }
    var editingGroup by remember { mutableStateOf<MobileBotSummaryAndroid?>(null) }
    var editingGroupMemberIds by remember { mutableStateOf(setOf<String>()) }
    var botName by remember { mutableStateOf("") }
    var botDescription by remember { mutableStateOf("") }
    var editingBotId by remember { mutableStateOf<String?>(null) }
    var deleteTarget by remember { mutableStateOf<AgentDeleteTarget?>(null) }
    var sectionMenuId by remember { mutableStateOf<String?>(null) }
    var renameSectionId by remember { mutableStateOf<String?>(null) }
    var renameSectionName by remember { mutableStateOf("") }
    var deleteSectionId by remember { mutableStateOf<String?>(null) }
    var showHiddenBots by remember { mutableStateOf(false) }

    LaunchedEffect(shortcutNewAgentNonce) {
        if (shortcutNewAgentNonce > 0) {
            addOpen = false
            createGroupOpen = false
            createOpen = true
        }
    }

    LaunchedEffect(hiddenChatsAvailability) {
        if (!hiddenChatsAvailability.isOpenable) showHiddenBots = false
    }

    HiddenBotsDialog(
        open = showHiddenBots && hiddenChatsAvailability.isOpenable,
        hiddenBots = botState.bots.filter { it.isHidden },
        pendingMutationIds = botState.hiddenMutationPendingIds,
        onClose = { showHiddenBots = false },
        onOpenBot = { bot ->
            showHiddenBots = false
            onOpenBot(bot)
        },
        onUnhide = onUnhideBot,
    )

    LaunchedEffect(
        query,
        addOpen,
        createOpen,
        createGroupOpen,
        groupName,
        groupDescription,
        groupMemberIds,
        editingGroup,
        editingGroupMemberIds,
        botName,
        botDescription,
        botState.creating,
        botState.error,
        botState.bots,
        botState.rosterLoading,
        showHiddenBots,
        messagingState.conversations,
        agentNetworkAvailability,
        hiddenChatsAvailability,
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
                    enabled = agentNetworkAvailability.isOpenable,
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) {
                        if (agentNetworkAvailability.isOpenable) onOpenAgentNetwork()
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
                    element(
                        "grok-mobile-new-agent-group",
                        "menuitem",
                        "New Agent group",
                        enabled = botState.bots.any { !it.isGroup },
                        action = FabushiAppAgentSurface.Action(setOf("invoke")) {
                            if (botState.bots.any { !it.isGroup }) {
                                addOpen = false
                                createGroupOpen = true
                                groupMemberIds = emptySet()
                            }
                        },
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
                            enabled = rosterAccessReadiness.isLoaded && rosterAccessReadiness.isConnected,
                            action = FabushiAppAgentSurface.Action(setOf("invoke")) {
                                if (rosterAccessReadiness.isLoaded && rosterAccessReadiness.isConnected) onOpenBot(bot)
                            },
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

    if (createGroupOpen) {
        val candidates = botState.bots.filter { !it.isGroup }
        AlertDialog(
            onDismissRequest = { if (!botState.creating) createGroupOpen = false },
            title = { Text("New Agent group") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(groupName, { groupName = it }, label = { Text("Group name") }, singleLine = true)
                    OutlinedTextField(groupDescription, { groupDescription = it }, label = { Text("Description") }, minLines = 2, maxLines = 4)
                    Text("Members ${groupMemberIds.size}/6", color = GrokMobileMuted, fontSize = 12.sp)
                    candidates.take(100).forEach { candidate ->
                        val selected = candidate.id in groupMemberIds
                        TextButton(
                            onClick = {
                                groupMemberIds = if (selected) groupMemberIds - candidate.id
                                else if (groupMemberIds.size < 6) groupMemberIds + candidate.id
                                else groupMemberIds
                            },
                            modifier = Modifier.fillMaxWidth().testTag("agent-group-member-${candidate.id}"),
                        ) {
                            Text(if (selected) "✓ ${candidate.name}" else candidate.name)
                        }
                    }
                    botState.error?.let { Text(it, color = Color(0xFFD14343), fontSize = 12.sp) }
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        onCreateGroup(groupName, groupDescription, groupMemberIds.toList()) {
                            groupName = ""
                            groupDescription = ""
                            groupMemberIds = emptySet()
                            createGroupOpen = false
                        }
                    },
                    enabled = groupName.isNotBlank() && groupMemberIds.isNotEmpty() && !botState.creating,
                    modifier = Modifier.testTag("create-agent-group-submit"),
                ) { Text(if (botState.creating) "Creating…" else "Create group") }
            },
            dismissButton = { TextButton(onClick = { createGroupOpen = false }, enabled = !botState.creating) { Text("Cancel") } },
        )
    }

    editingGroup?.let { group ->
        val candidates = botState.bots.filter { !it.isGroup }
        val groupMembersPending = botState.groupMembersUpdatingId == group.id
        AlertDialog(
            onDismissRequest = { if (!groupMembersPending) editingGroup = null },
            title = { Text("Members · ${group.name}") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Select 1–6 Agents", color = GrokMobileMuted, fontSize = 12.sp)
                    candidates.take(100).forEach { candidate ->
                        val selected = candidate.id in editingGroupMemberIds
                        TextButton(
                            onClick = {
                                editingGroupMemberIds = if (selected) editingGroupMemberIds - candidate.id
                                else if (editingGroupMemberIds.size < 6) editingGroupMemberIds + candidate.id
                                else editingGroupMemberIds
                            },
                            modifier = Modifier.fillMaxWidth().testTag("edit-agent-group-member-${candidate.id}"),
                        ) {
                            Text(if (selected) "✓ ${candidate.name}" else candidate.name)
                        }
                    }
                }
            },
            confirmButton = {
                Button(
                    onClick = {
                        if (editingGroupMemberIds.isNotEmpty() && !groupMembersPending) {
                            onSetGroupMembers(group.id, editingGroupMemberIds.toList()) {
                                editingGroup = null
                            }
                        }
                    },
                    enabled = editingGroupMemberIds.isNotEmpty() && !groupMembersPending,
                    modifier = Modifier.testTag("edit-agent-group-submit"),
                ) { Text(if (groupMembersPending) "Saving…" else "Save") }
            },
            dismissButton = {
                TextButton(
                    onClick = { editingGroup = null },
                    enabled = !groupMembersPending,
                ) { Text("Cancel") }
            },
        )
    }

    renameSectionId?.let { sectionId ->
        AlertDialog(
            onDismissRequest = { renameSectionId = null },
            title = { Text("Rename section") },
            text = {
                OutlinedTextField(
                    value = renameSectionName,
                    onValueChange = { renameSectionName = it },
                    label = { Text("Section name") },
                    singleLine = true,
                    modifier = Modifier.testTag("agent-section-rename-input"),
                )
            },
            confirmButton = {
                TextButton(
                    enabled = renameSectionName.trim().isNotEmpty(),
                    onClick = {
                        onRenameSidebarSection(sectionId, renameSectionName)
                        renameSectionId = null
                    },
                ) { Text("Save") }
            },
            dismissButton = { TextButton(onClick = { renameSectionId = null }) { Text("Cancel") } },
        )
    }

    deleteSectionId?.let { sectionId ->
        val section = botState.sidebarSnapshot.sections.firstOrNull { it.id == sectionId }
        if (section != null) {
            val copy = agentSidebarSectionDeleteCopy(section.name)
            AlertDialog(
                onDismissRequest = { deleteSectionId = null },
                title = { Text(copy.title) },
                text = { Text(copy.description) },
                confirmButton = {
                    TextButton(
                        onClick = {
                            onDeleteSidebarSection(sectionId)
                            deleteSectionId = null
                        },
                        modifier = Modifier.testTag("agent-section-delete-confirm"),
                    ) { Text(copy.confirmLabel) }
                },
                dismissButton = { TextButton(onClick = { deleteSectionId = null }) { Text(copy.cancelLabel) } },
            )
        } else {
            LaunchedEffect(sectionId) { deleteSectionId = null }
        }
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
                        color = if (agentNetworkAvailability.isOpenable) {
                            GrokMobileInk
                        } else {
                            GrokMobileMuted.copy(alpha = 0.35f)
                        },
                        modifier = Modifier
                            .padding(horizontal = 8.dp)
                            .clickable(
                                enabled = agentNetworkAvailability.isOpenable,
                                onClick = onOpenAgentNetwork,
                            )
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
                            DropdownMenuItem(
                                text = { Text("New Agent group") },
                                enabled = botState.bots.any { !it.isGroup },
                                onClick = { addOpen = false; createGroupOpen = true; groupMemberIds = emptySet() },
                            )
                            DropdownMenuItem(text = { Text("New message") }, onClick = { addOpen = false; onOpenMessaging() })
                            DropdownMenuItem(text = { Text("New group") }, onClick = { addOpen = false; onOpenMessaging() })
                            DropdownMenuItem(text = { Text("New channel") }, onClick = { addOpen = false; onOpenMessaging() })
                        }
                    }
                }
            }
            if (accessProjection.mayShowAccessNotice) {
                item {
                    val notice = accountAccessNoticeCopy(accessProjection)
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 18.dp, vertical = 8.dp)
                            .background(Color(0xFFFFF1E8), RoundedCornerShape(18.dp))
                            .padding(16.dp)
                            .testTag("account-access-notice"),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Text(notice.title, color = GrokMobileInk, fontWeight = FontWeight.Bold)
                        Text(notice.body, color = GrokMobileMuted, fontSize = 12.sp)
                        notice.action?.let { action ->
                            Button(
                                onClick = onOpenAccessOnboarding,
                                modifier = Modifier.testTag("account-access-action"),
                            ) {
                                Text(action)
                            }
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
            if (botState.bots.any { it.isHidden }) {
                item {
                    TextButton(
                        onClick = {
                            if (hiddenChatsAvailability.isOpenable) showHiddenBots = true
                        },
                        enabled = hiddenChatsAvailability.isOpenable,
                        modifier = Modifier
                            .fillMaxWidth()
                            .testTag("hidden-bots-open"),
                    ) {
                        Text(
                            "Hidden Bots",
                            modifier = Modifier.fillMaxWidth(),
                        )
                    }
                }
            }
            val matchingBots = botState.bots.filter {
                query.isBlank() ||
                    it.name.contains(query.trim(), true) ||
                    it.description.contains(query.trim(), true)
            }
            val visibleBots = matchingBots.filter { !it.isHidden }
            val sidebarProjection = projectSidebarAgents(
                agents = visibleBots,
                pinnedAgentIds = botState.pinnedAgentIds,
                updatedAt = MobileBotSummaryAndroid::updatedAt,
            )
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
                            onShowHiddenBots = {
                                if (hiddenChatsAvailability.isOpenable) showHiddenBots = true
                            },
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
                val visibleById = sidebarProjection.recent.associateBy { it.id }
                val renderedSections: List<Pair<AndroidSidebarSection?, List<MobileBotSummaryAndroid>>> =
                    if (botState.sidebarSnapshot.sections.isEmpty()) {
                        listOf(null to (sidebarProjection.pinned + sidebarProjection.recent))
                    } else {
                        buildList {
                        if (sidebarProjection.pinned.isNotEmpty()) {
                            add(null to sidebarProjection.pinned)
                        }
                        addAll(
                            botState.sidebarSnapshot.sections.map { section ->
                                section to section.agentIds.mapNotNull(visibleById::get)
                            },
                        )
                    }
                }
                renderedSections
                    .filter { (_, bots) -> bots.isNotEmpty() || query.isBlank() }
                    .forEach { (section, sectionBots) ->
                        val sectionName = section?.name?.ifBlank {
                            if (section.id == AGENT_UNASSIGNED_SECTION_ID) "Unassigned" else "Section"
                        } ?: if (sectionBots.any { it.isPinned }) "Pinned" else "Recent"
                        val isCollapsed = section?.isCollapsed == true
                        item {
                            if (section == null) {
                                SectionLabelAndroid(sectionName + "  " + sectionBots.size)
                            } else {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    TextButton(
                                        onClick = { onToggleSidebarSection(section.id) },
                                        modifier = Modifier
                                            .weight(1f)
                                            .testTag("agent-section-header-" + section.id),
                                    ) {
                                        Text(
                                            text = (if (isCollapsed) "▸ " else "▾ ") + sectionName + "  " + sectionBots.size,
                                            color = GrokMobileMuted,
                                            fontSize = 12.sp,
                                            fontWeight = FontWeight.SemiBold,
                                            modifier = Modifier.fillMaxWidth(),
                                        )
                                    }
                                    if (section.id != AGENT_UNASSIGNED_SECTION_ID) {
                                        Box {
                                            TextButton(
                                                onClick = { sectionMenuId = section.id },
                                                modifier = Modifier.testTag("agent-section-menu-" + section.id),
                                            ) { Text("⋮") }
                                            DropdownMenu(
                                                expanded = sectionMenuId == section.id,
                                                onDismissRequest = { sectionMenuId = null },
                                            ) {
                                                DropdownMenuItem(
                                                    text = { Text("Rename") },
                                                    onClick = {
                                                        sectionMenuId = null
                                                        renameSectionName = section.name
                                                        renameSectionId = section.id
                                                    },
                                                )
                                                val editable = botState.sidebarSnapshot.sections.filterNot { it.id == AGENT_UNASSIGNED_SECTION_ID }
                                                val index = editable.indexOfFirst { it.id == section.id }
                                                DropdownMenuItem(
                                                    text = { Text("Move up") },
                                                    enabled = index > 0,
                                                    onClick = {
                                                        val target = editable.getOrNull(index - 1)
                                                        if (target != null) {
                                                            sectionMenuId = null
                                                            onMoveSidebarSection(section.id, target.id, AgentSidebarSectionMovePosition.BEFORE)
                                                        }
                                                    },
                                                )
                                                DropdownMenuItem(
                                                    text = { Text("Move down") },
                                                    enabled = index >= 0 && index < editable.lastIndex,
                                                    onClick = {
                                                        val target = editable.getOrNull(index + 1)
                                                        if (target != null) {
                                                            sectionMenuId = null
                                                            onMoveSidebarSection(section.id, target.id, AgentSidebarSectionMovePosition.AFTER)
                                                        }
                                                    },
                                                )
                                                DropdownMenuItem(
                                                    text = { Text("Delete") },
                                                    onClick = {
                                                        sectionMenuId = null
                                                        deleteSectionId = section.id
                                                    },
                                                )
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if (!isCollapsed) {
                        items(sectionBots, key = { it.id }) { bot ->
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
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        if (bot.isPinned) {
                                            val pinnedIndex = sidebarProjection.pinned.indexOfFirst { it.id == bot.id }
                                            val previousPinned = sidebarProjection.pinned.getOrNull(pinnedIndex - 1)
                                            val nextPinned = sidebarProjection.pinned.getOrNull(pinnedIndex + 1)
                                            TextButton(
                                                onClick = {
                                                    previousPinned?.let {
                                                        onReorderPinnedBot(bot.id, it.id, PinnedMovePosition.BEFORE)
                                                    }
                                                },
                                                enabled = previousPinned != null,
                                                modifier = Modifier
                                                    .semantics {
                                                        contentDescription = "Move ${bot.name} up in pinned Agents"
                                                    }
                                                    .testTag("agent-pinned-up-${bot.id}"),
                                            ) { Text("↑", fontSize = 12.sp) }
                                            TextButton(
                                                onClick = {
                                                    nextPinned?.let {
                                                        onReorderPinnedBot(bot.id, it.id, PinnedMovePosition.AFTER)
                                                    }
                                                },
                                                enabled = nextPinned != null,
                                                modifier = Modifier
                                                    .semantics {
                                                        contentDescription = "Move ${bot.name} down in pinned Agents"
                                                    }
                                                    .testTag("agent-pinned-down-${bot.id}"),
                                            ) { Text("↓", fontSize = 12.sp) }
                                        }
                                        if (bot.isGroup) {
                                            TextButton(
                                                onClick = {
                                                    editingGroup = bot
                                                    editingGroupMemberIds = bot.memberIds.toSet()
                                                },
                                                modifier = Modifier.testTag("agent-group-members-${bot.id}"),
                                            ) { Text("Members", fontSize = 11.sp) }
                                        }
                                        val currentSectionId = currentAgentSidebarSectionId(
                                            botState.sidebarSnapshot.sections,
                                            bot.id,
                                        )
                                        AgentRowActions(
                                            agentId = bot.id,
                                            agentName = bot.name,
                                            isGroup = bot.isGroup,
                                            isPinned = bot.isPinned,
                                            hasUnread = bot.hasUnread,
                                            isHidden = bot.isHidden,
                                            onEditName = { editingBotId = it },
                                            onEditProfile = {
                                                onBeginAgentSettings(bot.id)
                                                profileTarget = bot
                                            },
                                            onShowFullConversation = { onOpenBot(bot) },
                                            onShowAsyncTasks = { onShowBotAsyncTasks(bot) },
                                            sections = botState.sidebarSnapshot.sections,
                                            currentSectionId = currentSectionId,
                                            onMoveToSection = onMoveBotToSection,
                                            onMoveToNewSection = onMoveBotToNewSection,
                                            onHideFromSidebar = onHideBot,
                                            onDuplicateAgent = onDuplicateBot,
                                            onTogglePin = onSetBotPinned,
                                            onSetAgentUnread = onSetBotUnread,
                                            onRequestDelete = { deleteTarget = it },
                                        )
                                    }
                                },
                            )
                        }
                        }
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

        val profileTargetId = profileTarget?.id
        DisposableEffect(profileTargetId) {
            onDispose {
                profileTargetId?.let(onEndAgentSettings)
            }
        }
        LaunchedEffect(profileTargetId, botState.bots) {
            val currentId = profileTargetId ?: return@LaunchedEffect
            val authoritative = botState.bots.firstOrNull { it.id == currentId }
            if (authoritative == null) {
                profileTarget = null
            } else if (authoritative != profileTarget) {
                profileTarget = authoritative
            }
        }

        AgentProfileEditor(
            agent = profileTarget,
            onClose = { profileTarget = null },
            onConfirm = { id, name, title, description, avatarShape, avatarColor ->
                onUpdateBotProfile(id, name, title, description, avatarShape, avatarColor)
                profileTarget = null
            },
            onSetNotifications = onSetBotNotifyOnUpdates,
        )

        AgentDeleteConfirmation(
            agent = deleteTarget,
            onClose = { deleteTarget = null },
            onConfirm = onDeleteBot,
        )
    }
}



internal const val ACCESS_ONBOARDING_URL = "https://fabushi.ombhrum.com/"

internal data class AccountAccessNoticeCopy(
    val title: String,
    val body: String,
    val action: String?,
)

internal fun accountAccessNoticeCopy(access: AccountAccessProjection): AccountAccessNoticeCopy =
    when (access.blockReason) {
        AccountAccessBlockReason.TEAM_PRIVACY_MODE -> AccountAccessNoticeCopy(
            title = "Your team's privacy mode blocks Fabushi",
            body = "Fabushi cannot run under the team's legacy privacy mode. Ask a team admin to change that policy.",
            action = "See Details",
        )
        AccountAccessBlockReason.TEAM_SETUP_REQUIRED -> AccountAccessNoticeCopy(
            title = "Your team has not set up Fabushi yet",
            body = "A team admin must finish setup before members can send messages.",
            action = "See Details",
        )
        AccountAccessBlockReason.TEAM_ACCESS_REQUIRED -> AccountAccessNoticeCopy(
            title = "Your team has not granted this account Fabushi access",
            body = "A team admin can grant access from the team's settings.",
            action = "Request Access",
        )
        AccountAccessBlockReason.NOT_OFFERED -> AccountAccessNoticeCopy(
            title = "Fabushi is not available for this account",
            body = "There is no setup or purchase path available for this account.",
            action = null,
        )
        AccountAccessBlockReason.FREE_TRIAL_AVAILABLE -> AccountAccessNoticeCopy(
            title = "Start a Fabushi trial to send messages",
            body = "This account can start a trial now.",
            action = "Start Trial",
        )
        AccountAccessBlockReason.PAYWALL_INDIVIDUAL -> AccountAccessNoticeCopy(
            title = "Fabushi requires an eligible plan",
            body = "Upgrade this account before sending messages.",
            action = "Upgrade",
        )
        AccountAccessBlockReason.PAYWALL_TEAM_MEMBER -> AccountAccessNoticeCopy(
            title = "Fabushi requires an eligible team seat",
            body = "Ask a team admin to move this account to an eligible seat.",
            action = "Request Access",
        )
        AccountAccessBlockReason.PAYWALL_TEAM_ADMIN -> AccountAccessNoticeCopy(
            title = "Fabushi requires an eligible team seat",
            body = "Move this account to an eligible seat before sending messages.",
            action = "Manage Seats",
        )
        AccountAccessBlockReason.NONE,
        AccountAccessBlockReason.UNSPECIFIED -> when (access.sandAccessState) {
            AccountAccessState.UNAVAILABLE -> AccountAccessNoticeCopy(
                title = "Fabushi is not available for this account",
                body = "Sending stays disabled until this account is granted access.",
                action = "Check Access",
            )
            AccountAccessState.PAYMENT_REQUIRED -> AccountAccessNoticeCopy(
                title = "Fabushi is not included in this plan",
                body = "Sending stays disabled until the account has access.",
                action = "Check Access",
            )
            else -> AccountAccessNoticeCopy(
                title = "Fabushi is not available on this account yet",
                body = "Check what this account needs on the web.",
                action = "Check Access",
            )
        }
    }

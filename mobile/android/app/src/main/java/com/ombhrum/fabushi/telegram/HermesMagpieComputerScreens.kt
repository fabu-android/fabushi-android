package com.ombhrum.fabushi.telegram

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.ConversationKind
import com.ombhrum.fabushi.RemoteComputerSurface

// ============================================================================
// 1. Unified Apple Agents Hub (`智能体` Tab: Bots | Magpie AI | Hermes)
// ============================================================================

@Composable
fun BotFatherWorkspaceScreen(
    existingBots: List<TelegramChatItem>,
    fabushiBots: List<FabushiBotItem> = emptyList(),
    codingBots: List<CodingAgentBotItem> = emptyList(),
    codingProbes: List<CodingAgentProbeItem> = emptyList(),
    lastCreatedBotToken: String? = null,
    magpieState: MagpieGatewayUiState = MagpieGatewayUiState(),
    hermesState: HermesStudioUiState = HermesStudioUiState(),
    isAuthenticated: Boolean = false,
    onCreateBot: (name: String, description: String, handleHint: String) -> Unit,
    onBotFatherNlCreate: (prompt: String) -> Unit = {},
    onCreateCodingAgentBot: (
        provider: CodingProviderKindUi,
        name: String,
        handle: String,
        workspacePath: String,
        permissionMode: CodingPermissionModeUi,
        useWorktree: Boolean,
    ) -> Unit = { _, _, _, _, _, _ -> },
    onCycleCodingPermission: (botId: String, nextMode: CodingPermissionModeUi) -> Unit = { _, _ -> },
    onProbeCodingAgents: () -> Unit = {},
    onRefreshBots: () -> Unit = {},
    onOpenBotChat: (String) -> Unit,
    // Magpie callbacks
    onSetMagpieStrategy: (MagpieRoutingStrategy) -> Unit = {},
    onConfigureMagpieProvider: (String, String) -> Unit = { _, _ -> },
    onImportMagpieEnvOrUri: (String) -> Unit = {},
    onTestMagpiePiiMask: (String) -> Unit = {},
    onSyncModelsDev: () -> Unit = {},
    onScanLocalSubscriptions: () -> Unit = {},
    // Hermes callbacks
    onSelectHermesTab: (HermesStudioTab) -> Unit = {},
    onBranchHermesSession: (String) -> Unit = {},
    onRunHermesMoa: (String) -> Unit = {},
    onUpdateHermesSoul: (String) -> Unit = {},
    onToggleHermesSkill: (String) -> Unit = {},
    onExecuteHermesTerminal: (String) -> Unit = {},
    onAddHermesCron: (String, String, String) -> Unit = { _, _, _ -> },
    onAdvanceHermesKanban: (String) -> Unit = {},
    onDecideHermesApproval: (String, Boolean) -> Unit = { _, _ -> },
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    var activeSegment by remember { mutableIntStateOf(0) } // 0 = Bots, 1 = Magpie AI, 2 = Hermes

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground)
            .testTag("telegram-botfather-screen"),
    ) {
        // Apple Large Title + 3-Segment Switcher
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(palette.topBarBackground)
                .statusBarsPadding()
                .padding(horizontal = 16.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = strings.tabAgents,
                    color = palette.topBarContent,
                    fontSize = 28.sp,
                    fontWeight = FontWeight.Bold,
                )
                if (activeSegment == 0) {
                    TextButton(onClick = onRefreshBots) {
                        Text(
                            if (strings.isEnglish) "Refresh" else "刷新",
                            color = palette.topBarActionColor,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                }
            }
            AppleSegmentedControl(
                options = listOf(strings.agentsBotsTab, strings.agentsMagpieTab, strings.agentsHermesTab),
                selectedIndex = activeSegment,
                onSelect = { activeSegment = it },
            )
        }
        TelegramHairlineDivider()

        when (activeSegment) {
            0 -> BotsRegistryContent(
                existingBots = existingBots,
                fabushiBots = fabushiBots,
                codingBots = codingBots,
                codingProbes = codingProbes,
                lastCreatedBotToken = lastCreatedBotToken,
                isAuthenticated = isAuthenticated,
                onCreateBot = onCreateBot,
                onBotFatherNlCreate = onBotFatherNlCreate,
                onCreateCodingAgentBot = onCreateCodingAgentBot,
                onCycleCodingPermission = onCycleCodingPermission,
                onProbeCodingAgents = onProbeCodingAgents,
                onOpenBotChat = onOpenBotChat,
            )

            1 -> MagpieGatewayContent(
                gatewayState = magpieState,
                onSetStrategy = onSetMagpieStrategy,
                onConfigureProvider = onConfigureMagpieProvider,
                onImportEnvOrUri = onImportMagpieEnvOrUri,
                onTestPiiMask = onTestMagpiePiiMask,
                onSyncModelsDev = onSyncModelsDev,
                onScanLocalSubscriptions = onScanLocalSubscriptions,
            )

            else -> HermesStudioContent(
                hermesState = hermesState,
                onSelectTab = onSelectHermesTab,
                onBranchSession = onBranchHermesSession,
                onRunMoa = onRunHermesMoa,
                onUpdateSoul = onUpdateHermesSoul,
                onToggleSkill = onToggleHermesSkill,
                onExecuteTerminal = onExecuteHermesTerminal,
                onAddCron = onAddHermesCron,
                onAdvanceKanban = onAdvanceHermesKanban,
                onDecideApproval = onDecideHermesApproval,
            )
        }
    }
}

@Composable
private fun BotsRegistryContent(
    existingBots: List<TelegramChatItem>,
    fabushiBots: List<FabushiBotItem>,
    codingBots: List<CodingAgentBotItem>,
    codingProbes: List<CodingAgentProbeItem>,
    lastCreatedBotToken: String?,
    isAuthenticated: Boolean,
    onCreateBot: (String, String, String) -> Unit,
    onBotFatherNlCreate: (String) -> Unit,
    onCreateCodingAgentBot: (CodingProviderKindUi, String, String, String, CodingPermissionModeUi, Boolean) -> Unit,
    onCycleCodingPermission: (String, CodingPermissionModeUi) -> Unit,
    onProbeCodingAgents: () -> Unit,
    onOpenBotChat: (String) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val inputColors = appleFieldColors(palette)

    var nlPrompt by remember { mutableStateOf("") }
    var botName by remember { mutableStateOf("") }
    var botHandle by remember { mutableStateOf("") }
    var botDescription by remember { mutableStateOf("") }
    var codingProvider by remember { mutableStateOf(CodingProviderKindUi.CLAUDE_CODE) }
    var codingWorkspacePath by remember { mutableStateOf("") }
    var codingPermission by remember { mutableStateOf(CodingPermissionModeUi.ASK) }
    var codingUseWorktree by remember { mutableStateOf(true) }
    var showCreateForm by remember { mutableStateOf(false) }
    var showCodingForm by remember { mutableStateOf(false) }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        // Real Server Bots & Fabushi AI Bots List
        item {
            Text(
                text = if (strings.isEnglish) {
                    "My Bots (${fabushiBots.size.coerceAtLeast(existingBots.size)})"
                } else {
                    "我的智能体 (${fabushiBots.size.coerceAtLeast(existingBots.size)})"
                },
                color = palette.secondaryText,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
            )
        }

        if (fabushiBots.isEmpty() && existingBots.isEmpty()) {
            item {
                AppleCard {
                    Column(
                        modifier = Modifier.padding(18.dp),
                        verticalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        Text(
                            text = if (isAuthenticated) {
                                if (strings.isEnglish) "No bots created yet" else "尚未创建智能体"
                            } else {
                                if (strings.isEnglish) "Sign in to load your bots" else "登录服务器账号后同步您的智能体"
                            },
                            color = palette.primaryText,
                            fontWeight = FontWeight.SemiBold,
                            fontSize = 15.sp,
                        )
                        Text(
                            text = if (strings.isEnglish) {
                                "Create a bot with natural language or structured settings below."
                            } else {
                                "所有智能体均来自服务端真实数据，您可在下方一键自然语言创建或自定义配置。"
                            },
                            color = palette.secondaryText,
                            fontSize = 12.sp,
                        )
                    }
                }
            }
        } else if (fabushiBots.isNotEmpty()) {
            items(fabushiBots, key = { "${it.botUserId}-${it.agentId}-${it.username}" }) { bot ->
                val activityText = formatBotActivitySubtitle(
                    bot.executionState,
                    null,
                    if (strings.isEnglish) "en-US" else "zh-CN",
                )
                val isBusy = activityText != null
                AppleCard {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable {
                                val targetChatId = bot.companionChatId?.toString()
                                    ?: existingBots.firstOrNull {
                                        it.peerUserId == bot.botUserId ||
                                            matchesBotUsername(it.username, bot.username)
                                    }?.id
                                if (!targetChatId.isNullOrBlank()) {
                                    onOpenBotChat(targetChatId)
                                }
                            }
                            .padding(12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TelegramAvatar(
                            idSeed = bot.agentId.ifBlank { bot.botUserId.toString() },
                            title = bot.displayName.ifBlank { bot.username },
                            size = 46.dp,
                            avatarShape = bot.avatarShape,
                            avatarColor = bot.avatarColor,
                            isBot = true,
                            botActivityActive = isBusy,
                        )
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = bot.displayName.ifBlank { bot.username },
                                color = palette.primaryText,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            val handle = bot.username.takeIf { it.isNotBlank() }?.let { if (it.startsWith("@")) it else "@$it" }.orEmpty()
                            Text(
                                text = activityText ?: listOfNotNull(
                                    handle.takeIf { it.isNotBlank() },
                                    bot.preferredModel.takeIf { it.isNotBlank() },
                                ).joinToString(" · ").ifBlank { if (strings.isEnglish) "Idle" else "空闲" },
                                color = if (isBusy) palette.statusGreenText else palette.secondaryText,
                                fontSize = 12.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 14.dp)
                    }
                }
            }
        } else {
            items(existingBots, key = { it.id }) { bot ->
                val activityText = formatBotActivitySubtitle(
                    bot.botExecutionState,
                    bot.botActivityDetail,
                    if (strings.isEnglish) "en-US" else "zh-CN",
                )
                AppleCard {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { onOpenBotChat(bot.id) }
                            .padding(12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TelegramAvatar(
                            idSeed = bot.id,
                            title = bot.title,
                            size = 46.dp,
                            online = bot.online,
                            avatarShape = bot.avatarShape,
                            avatarColor = bot.avatarColor,
                            isBot = true,
                            botActivityActive = activityText != null,
                        )
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(bot.title, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                            Text(
                                text = activityText ?: (bot.username ?: bot.subtitle).ifBlank { if (strings.isEnglish) "Idle" else "空闲" },
                                color = if (activityText != null) palette.statusGreenText else palette.secondaryText,
                                fontSize = 12.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 14.dp)
                    }
                }
            }
        }

        if (!lastCreatedBotToken.isNullOrBlank()) {
            item {
                AppleCard {
                    Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text("Bot Token (Server Issued)", color = palette.statusGreenText, fontWeight = FontWeight.SemiBold, fontSize = 12.sp)
                        Text(lastCreatedBotToken, color = palette.primaryText, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                    }
                }
            }
        }

        // Natural Language BotFather Creation
        item {
            AppleCard {
                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text(
                        text = if (strings.isEnglish) "Quick Create via Natural Language" else "一句话创建智能体 (BotFather)",
                        color = palette.primaryText,
                        fontWeight = FontWeight.SemiBold,
                        fontSize = 15.sp,
                    )
                    OutlinedTextField(
                        value = nlPrompt,
                        onValueChange = { nlPrompt = it },
                        placeholder = {
                            Text(
                                if (strings.isEnglish) "Describe the bot's role, tools, and style..." else "描述你想要的智能体角色、能力与风格…",
                                color = palette.hintText,
                            )
                        },
                        minLines = 2,
                        colors = inputColors,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(
                        enabled = nlPrompt.isNotBlank(),
                        onClick = {
                            onBotFatherNlCreate(nlPrompt.trim())
                            nlPrompt = ""
                        },
                        colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                        shape = RoundedCornerShape(10.dp),
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Text(if (strings.isEnglish) "Create with BotFather" else "创建智能体")
                    }
                }
            }
        }

        // Structured Bot Creation (empty inputs, zero pre-filled demo data!)
        item {
            AppleCard {
                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clickable { showCreateForm = !showCreateForm },
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = if (strings.isEnglish) "Create Bot Manually" else "手动创建自定义智能体",
                            color = palette.primaryText,
                            fontWeight = FontWeight.SemiBold,
                            fontSize = 15.sp,
                        )
                        TelegramIcon(
                            glyph = if (showCreateForm) TelegramGlyph.CHEVRON_DOWN else TelegramGlyph.CHEVRON_RIGHT,
                            tint = palette.secondaryText,
                            size = 16.dp,
                        )
                    }
                    if (showCreateForm) {
                        OutlinedTextField(
                            value = botName,
                            onValueChange = { botName = it },
                            label = { Text(if (strings.isEnglish) "Display Name" else "智能体名称") },
                            singleLine = true,
                            colors = inputColors,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        OutlinedTextField(
                            value = botHandle,
                            onValueChange = { botHandle = it },
                            label = { Text(if (strings.isEnglish) "Username (e.g. my_assistant_bot)" else "用户名 (如 my_assistant_bot)") },
                            singleLine = true,
                            colors = inputColors,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        OutlinedTextField(
                            value = botDescription,
                            onValueChange = { botDescription = it },
                            label = { Text(if (strings.isEnglish) "Description" else "简介说明") },
                            minLines = 2,
                            colors = inputColors,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Button(
                            enabled = botName.isNotBlank(),
                            onClick = {
                                onCreateBot(botName.trim(), botDescription.trim(), botHandle.trim())
                                botName = ""
                                botHandle = ""
                                botDescription = ""
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                            shape = RoundedCornerShape(10.dp),
                            modifier = Modifier
                                .fillMaxWidth()
                                .testTag("botfather-create-button"),
                        ) {
                            Text(if (strings.isEnglish) "Create Bot" else "提交创建")
                        }
                    }
                }
            }
        }

        // Coding Agents & Probe Card
        item {
            AppleCard(modifier = Modifier.testTag("botfather-coding-agent-card")) {
                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = if (strings.isEnglish) "Coding Agents" else "编程智能体 (Coding Agents)",
                            color = palette.primaryText,
                            fontWeight = FontWeight.SemiBold,
                            fontSize = 15.sp,
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            TextButton(onClick = onProbeCodingAgents) {
                                Text(if (strings.isEnglish) "Probe" else "探测环境", color = palette.topBarActionColor, fontSize = 12.sp)
                            }
                            TextButton(onClick = { showCodingForm = !showCodingForm }) {
                                Text(if (showCodingForm) strings.done else if (strings.isEnglish) "New" else "新建", color = palette.topBarActionColor, fontSize = 12.sp)
                            }
                        }
                    }

                    if (codingProbes.isNotEmpty()) {
                        LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            itemsIndexed(codingProbes, key = { index, probe -> "probe_${index}_${probe.providerKind.wire}" }) { _, probe ->
                                Surface(
                                    shape = RoundedCornerShape(8.dp),
                                    color = if (probe.installed) palette.secretGreen.copy(alpha = 0.14f) else palette.inputBg,
                                ) {
                                    Text(
                                        text = "${probe.providerKind.label}: ${if (probe.installed) probe.version ?: "OK" else "未安装"}",
                                        color = if (probe.installed) palette.statusGreenText else palette.secondaryText,
                                        fontSize = 11.sp,
                                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 5.dp),
                                    )
                                }
                            }
                        }
                    }

                    if (codingBots.isNotEmpty()) {
                        codingBots.forEach { cb ->
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clip(RoundedCornerShape(10.dp))
                                    .background(palette.cardBg)
                                    .padding(10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Column(Modifier.weight(1f)) {
                                    Text(cb.displayName, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                    Text(
                                        "${cb.provider.label} · ${cb.workspacePath.ifBlank { "~" }} · ${cb.permissionMode.label}",
                                        color = palette.secondaryText,
                                        fontSize = 11.sp,
                                    )
                                }
                                TextButton(
                                    onClick = {
                                        val modes = CodingPermissionModeUi.entries
                                        val next = modes[(modes.indexOf(cb.permissionMode) + 1) % modes.size]
                                        onCycleCodingPermission(cb.botId, next)
                                    },
                                ) {
                                    Text(cb.permissionMode.label, color = palette.primaryAccent, fontSize = 11.sp)
                                }
                            }
                        }
                    }

                    if (showCodingForm) {
                        LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            items(CodingProviderKindUi.entries) { kind ->
                                val selected = codingProvider == kind
                                Surface(
                                    shape = RoundedCornerShape(10.dp),
                                    color = if (selected) palette.selectedPillBackground else palette.inputBg,
                                    modifier = Modifier
                                        .clip(RoundedCornerShape(10.dp))
                                        .clickable { codingProvider = kind },
                                ) {
                                    Text(
                                        text = kind.label,
                                        color = if (selected) palette.selectedPillContent else palette.primaryText,
                                        fontSize = 12.sp,
                                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                    )
                                }
                            }
                        }
                        OutlinedTextField(
                            value = codingWorkspacePath,
                            onValueChange = { codingWorkspacePath = it },
                            label = { Text(if (strings.isEnglish) "Workspace Path" else "工作区目录") },
                            placeholder = { Text("~/workspace", color = palette.hintText) },
                            singleLine = true,
                            colors = inputColors,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                            CodingPermissionModeUi.entries.forEach { mode ->
                                val selected = codingPermission == mode
                                Surface(
                                    shape = RoundedCornerShape(8.dp),
                                    color = if (selected) palette.selectedPillBackground else palette.inputBg,
                                    modifier = Modifier
                                        .clip(RoundedCornerShape(8.dp))
                                        .clickable { codingPermission = mode },
                                ) {
                                    Text(
                                        text = mode.label,
                                        color = if (selected) palette.selectedPillContent else palette.primaryText,
                                        fontSize = 11.sp,
                                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                    )
                                }
                            }
                        }
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                text = if (strings.isEnglish) "Use Git Worktree Isolation" else "启用 Git Worktree 隔离",
                                color = palette.primaryText,
                                fontSize = 13.sp,
                                modifier = Modifier.weight(1f),
                            )
                            Switch(checked = codingUseWorktree, onCheckedChange = { codingUseWorktree = it })
                        }
                        Button(
                            onClick = {
                                val resolvedName = botName.trim().ifBlank { codingProvider.label }
                                val resolvedHandle = botHandle.trim().ifBlank { "${codingProvider.wire}_bot" }
                                onCreateCodingAgentBot(
                                    codingProvider,
                                    resolvedName,
                                    resolvedHandle,
                                    codingWorkspacePath.trim(),
                                    codingPermission,
                                    codingUseWorktree,
                                )
                                showCodingForm = false
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                            shape = RoundedCornerShape(10.dp),
                            modifier = Modifier
                                .fillMaxWidth()
                                .testTag("botfather-create-coding-bot"),
                        ) {
                            Text(if (strings.isEnglish) "Create Coding Agent Bot" else "创建编程智能体")
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// 2. Honest Magpie AI Gateway Screen & Embedded Content
// ============================================================================

@Composable
fun MagpieGatewayScreen(
    gatewayState: MagpieGatewayUiState,
    onSetStrategy: (MagpieRoutingStrategy) -> Unit,
    onConfigureProvider: (String, String) -> Unit,
    onImportEnvOrUri: (String) -> Unit,
    onTestPiiMask: (String) -> Unit,
    onSyncModelsDev: () -> Unit = {},
    onScanLocalSubscriptions: () -> Unit = {},
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground)
            .testTag("telegram-magpie-gateway-screen"),
    ) {
        WorkspaceTopBar(
            title = "Magpie AI Gateway",
            subtitle = if (gatewayState.liveReady) "已配置实时通道" else "未配置 API Key",
            glyph = TelegramGlyph.MAGPIE_GATEWAY,
            onBack = onBack,
        )
        MagpieGatewayContent(
            gatewayState = gatewayState,
            onSetStrategy = onSetStrategy,
            onConfigureProvider = onConfigureProvider,
            onImportEnvOrUri = onImportEnvOrUri,
            onTestPiiMask = onTestPiiMask,
            onSyncModelsDev = onSyncModelsDev,
            onScanLocalSubscriptions = onScanLocalSubscriptions,
        )
    }
}

@Composable
private fun MagpieGatewayContent(
    gatewayState: MagpieGatewayUiState,
    onSetStrategy: (MagpieRoutingStrategy) -> Unit,
    onConfigureProvider: (String, String) -> Unit,
    onImportEnvOrUri: (String) -> Unit,
    onTestPiiMask: (String) -> Unit,
    onSyncModelsDev: () -> Unit,
    onScanLocalSubscriptions: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val inputColors = appleFieldColors(palette)

    var searchQuery by remember { mutableStateOf("") }
    var editingPreset by remember { mutableStateOf<MagpieProviderPreset?>(null) }
    var keyDraft by remember { mutableStateOf("") }
    var importDraft by remember { mutableStateOf("") }
    var piiDraft by remember { mutableStateOf("") }

    val configuredCount = gatewayState.presets.count { it.configured }
    val filteredPresets = remember(gatewayState.presets, searchQuery) {
        if (searchQuery.isBlank()) gatewayState.presets
        else gatewayState.presets.filter {
            it.name.contains(searchQuery, ignoreCase = true) ||
                it.id.contains(searchQuery, ignoreCase = true) ||
                it.flagshipModel.contains(searchQuery, ignoreCase = true)
        }
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        // Honest Live Readiness Card
        item {
            AppleCard {
                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("Magpie AI Gateway", color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = if (gatewayState.liveReady) palette.secretGreen.copy(alpha = 0.16f) else palette.inputBg,
                        ) {
                            Text(
                                text = if (gatewayState.liveReady) "已配置 $configuredCount 个通道" else "未配置 API Key",
                                color = if (gatewayState.liveReady) palette.statusGreenText else palette.secondaryText,
                                fontSize = 11.sp,
                                fontWeight = FontWeight.SemiBold,
                                modifier = Modifier.padding(horizontal = 9.dp, vertical = 4.dp),
                            )
                        }
                    }
                    val totalTokens = gatewayState.totalPromptTokens + gatewayState.totalCompletionTokens
                    val tokenLabel = if (totalTokens >= 1000) "${totalTokens / 1000}K" else "$totalTokens"
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        GatewayMetric("请求次数", "${gatewayState.totalRequests}")
                        GatewayMetric("Token 消耗", tokenLabel)
                        GatewayMetric("估算费用", "$${"%.4f".format(gatewayState.totalCostUsd)}")
                    }
                }
            }
        }

        // Models.dev Sync & Local Subscription Scan
        item {
            AppleCard(modifier = Modifier.testTag("magpie-models-dev-sync-card")) {
                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("模型目录同步 (models.dev)", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                    Text(
                        text = if (gatewayState.discoveredModels.isNotEmpty()) {
                            "已同步 ${gatewayState.discoveredModels.size} 个模型 · ${gatewayState.modelsDevStatusLabel}"
                        } else {
                            gatewayState.modelsDevStatusLabel
                        },
                        color = palette.secondaryText,
                        fontSize = 12.sp,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Button(
                            onClick = onSyncModelsDev,
                            colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                            shape = RoundedCornerShape(10.dp),
                            modifier = Modifier.weight(1f),
                        ) {
                            Text("同步目录", fontSize = 13.sp)
                        }
                        OutlinedButton(
                            onClick = onScanLocalSubscriptions,
                            shape = RoundedCornerShape(10.dp),
                            modifier = Modifier.weight(1f),
                        ) {
                            Text("检测本机配置", color = palette.primaryAccent, fontSize = 13.sp)
                        }
                    }
                }
            }
        }

        // Routing Strategy
        item {
            LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                items(MagpieRoutingStrategy.entries) { strategy ->
                    val selected = gatewayState.strategy == strategy
                    Surface(
                        shape = RoundedCornerShape(10.dp),
                        color = if (selected) palette.selectedPillBackground else palette.modalBg,
                        modifier = Modifier
                            .clip(RoundedCornerShape(10.dp))
                            .clickable { onSetStrategy(strategy) },
                    ) {
                        Text(
                            text = strategy.label,
                            color = if (selected) palette.selectedPillContent else palette.primaryText,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Medium,
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 8.dp),
                        )
                    }
                }
            }
        }

        // Bulk Import (.env / URI) & PII Masking
        item {
            AppleCard {
                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("批量导入 (.env / URI)", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                    OutlinedTextField(
                        value = importDraft,
                        onValueChange = { importDraft = it },
                        placeholder = { Text("OPENAI_API_KEY=sk-... 或 magpie://...", color = palette.hintText) },
                        minLines = 2,
                        colors = inputColors,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Button(
                        enabled = importDraft.isNotBlank(),
                        onClick = {
                            onImportEnvOrUri(importDraft.trim())
                            importDraft = ""
                        },
                        colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                        shape = RoundedCornerShape(10.dp),
                    ) {
                        Text("解析导入")
                    }
                    TelegramHairlineDivider()
                    Text("敏感信息脱敏验证 (PII Masking)", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                        OutlinedTextField(
                            value = piiDraft,
                            onValueChange = { piiDraft = it },
                            placeholder = { Text("输入测试文本验证密钥/邮箱脱敏…", color = palette.hintText) },
                            singleLine = true,
                            colors = inputColors,
                            modifier = Modifier.weight(1f),
                        )
                        OutlinedButton(
                            enabled = piiDraft.isNotBlank(),
                            onClick = { onTestPiiMask(piiDraft) },
                        ) {
                            Text("测试", color = palette.primaryAccent)
                        }
                    }
                    if (gatewayState.lastMaskedPreview.isNotBlank()) {
                        Text(gatewayState.lastMaskedPreview, color = palette.secondaryText, fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                    }
                }
            }
        }

        // Provider Presets Search & List
        item {
            OutlinedTextField(
                value = searchQuery,
                onValueChange = { searchQuery = it },
                placeholder = { Text("搜索 48 个模型通道 (OpenAI, Claude, DeepSeek…)", color = palette.hintText) },
                singleLine = true,
                colors = inputColors,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        items(filteredPresets, key = { it.id }) { preset ->
            AppleCard {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable {
                            editingPreset = preset
                            keyDraft = ""
                        }
                        .padding(horizontal = 14.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(preset.name, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                        Text("${preset.flagshipModel} · ${preset.region}", color = palette.secondaryText, fontSize = 11.sp)
                    }
                    Surface(
                        shape = RoundedCornerShape(8.dp),
                        color = if (preset.configured) palette.secretGreen.copy(alpha = 0.16f) else palette.inputBg,
                    ) {
                        Text(
                            text = if (preset.configured) "已配置" else "未配置",
                            color = if (preset.configured) palette.statusGreenText else palette.secondaryText,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                        )
                    }
                }
            }
        }
    }

    editingPreset?.let { preset ->
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { editingPreset = null },
            containerColor = palette.modalBg,
            titleContentColor = palette.primaryText,
            textContentColor = palette.primaryText,
            title = { Text("配置 ${preset.name}") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("默认模型：${preset.flagshipModel}", color = palette.secondaryText, fontSize = 12.sp)
                    OutlinedTextField(
                        value = keyDraft,
                        onValueChange = { keyDraft = it },
                        label = { Text("API Key") },
                        singleLine = true,
                        colors = inputColors,
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            },
            confirmButton = {
                Button(
                    enabled = keyDraft.isNotBlank(),
                    onClick = {
                        onConfigureProvider(preset.id, keyDraft.trim())
                        editingPreset = null
                    },
                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                ) {
                    Text(strings.save)
                }
            },
            dismissButton = {
                TextButton(onClick = { editingPreset = null }) {
                    Text(strings.cancel, color = palette.topBarActionColor)
                }
            },
        )
    }
}

// ============================================================================
// 3. Honest Hermes Studio Screen & Embedded Content
// ============================================================================

@Composable
fun HermesStudioScreen(
    hermesState: HermesStudioUiState,
    onSelectTab: (HermesStudioTab) -> Unit,
    onBranchSession: (String) -> Unit,
    onRunMoa: (String) -> Unit,
    onUpdateSoul: (String) -> Unit,
    onToggleSkill: (String) -> Unit,
    onExecuteTerminal: (String) -> Unit,
    onAddCron: (String, String, String) -> Unit,
    onAdvanceKanban: (String) -> Unit,
    onDecideApproval: (String, Boolean) -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)
    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground),
    ) {
        WorkspaceTopBar(
            title = "Hermes Studio",
            subtitle = "记忆、技能、终端与审批",
            glyph = TelegramGlyph.HERMES_SPARK,
            onBack = onBack,
        )
        HermesStudioContent(
            hermesState = hermesState,
            onSelectTab = onSelectTab,
            onBranchSession = onBranchSession,
            onRunMoa = onRunMoa,
            onUpdateSoul = onUpdateSoul,
            onToggleSkill = onToggleSkill,
            onExecuteTerminal = onExecuteTerminal,
            onAddCron = onAddCron,
            onAdvanceKanban = onAdvanceKanban,
            onDecideApproval = onDecideApproval,
        )
    }
}

@Composable
private fun HermesStudioContent(
    hermesState: HermesStudioUiState,
    onSelectTab: (HermesStudioTab) -> Unit,
    onBranchSession: (String) -> Unit,
    onRunMoa: (String) -> Unit,
    onUpdateSoul: (String) -> Unit,
    onToggleSkill: (String) -> Unit,
    onExecuteTerminal: (String) -> Unit,
    onAddCron: (String, String, String) -> Unit,
    onAdvanceKanban: (String) -> Unit,
    onDecideApproval: (String, Boolean) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val inputColors = appleFieldColors(palette)

    var moaPrompt by remember { mutableStateOf("") }
    var soulDraft by remember(hermesState.soulMarkdown) { mutableStateOf(hermesState.soulMarkdown) }
    var terminalInput by remember { mutableStateOf("") }
    var cronTitle by remember { mutableStateOf("") }
    var cronExpr by remember { mutableStateOf("") }
    var cronPrompt by remember { mutableStateOf("") }

    Column(modifier = Modifier.fillMaxSize()) {
        LazyRow(
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 10.dp),
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            items(HermesStudioTab.entries) { tab ->
                val selected = hermesState.activeTab == tab
                Surface(
                    shape = RoundedCornerShape(10.dp),
                    color = if (selected) palette.selectedPillBackground else palette.modalBg,
                    modifier = Modifier
                        .clip(RoundedCornerShape(10.dp))
                        .clickable { onSelectTab(tab) },
                ) {
                    Text(
                        text = tab.title,
                        color = if (selected) palette.selectedPillContent else palette.primaryText,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 7.dp),
                    )
                }
            }
        }

        LazyColumn(
            modifier = Modifier.weight(1f),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            when (hermesState.activeTab) {
                HermesStudioTab.SESSIONS_PROVIDERS -> {
                    item {
                        AppleCard {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("多模型协作推理 (MoA)", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                OutlinedTextField(
                                    value = moaPrompt,
                                    onValueChange = { moaPrompt = it },
                                    placeholder = { Text("输入问题（需先在 Magpie 配置 API Key）…", color = palette.hintText) },
                                    minLines = 2,
                                    colors = inputColors,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                                Button(
                                    enabled = moaPrompt.isNotBlank(),
                                    onClick = {
                                        onRunMoa(moaPrompt.trim())
                                        moaPrompt = ""
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                ) {
                                    Text("运行推理")
                                }
                                val activeSession = hermesState.sessions.firstOrNull { it.id == hermesState.activeSessionId }
                                    ?: hermesState.sessions.firstOrNull()
                                val latestOut = activeSession?.lastOutput.orEmpty()
                                if (latestOut.isNotBlank()) {
                                    Text(latestOut, color = palette.primaryText, fontSize = 13.sp)
                                }
                            }
                        }
                    }
                    if (hermesState.sessions.isEmpty()) {
                        item { HonestEmptyCard("暂无 Hermes 会话记录") }
                    } else {
                        items(hermesState.sessions, key = { it.id }) { session ->
                            AppleCard {
                                Row(
                                    modifier = Modifier.padding(12.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        Text(session.title, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                        Text("${session.provider} · ${session.model} · ${session.turnCount} 轮", color = palette.secondaryText, fontSize = 11.sp)
                                    }
                                    TextButton(onClick = { onBranchSession(session.id) }) {
                                        Text("分支", color = palette.topBarActionColor)
                                    }
                                }
                            }
                        }
                    }
                }

                HermesStudioTab.MEMORY_SOUL -> {
                    item {
                        AppleCard {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("SOUL.md 人格与核心指令", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                OutlinedTextField(
                                    value = soulDraft,
                                    onValueChange = { soulDraft = it },
                                    placeholder = { Text("# SOUL.md\n在此编写真实的人格与行为准则…", color = palette.hintText) },
                                    minLines = 4,
                                    colors = inputColors,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                                Button(
                                    enabled = soulDraft.isNotBlank(),
                                    onClick = { onUpdateSoul(soulDraft) },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                ) {
                                    Text(strings.save)
                                }
                            }
                        }
                    }
                    if (hermesState.memoryBlocks.isEmpty()) {
                        item { HonestEmptyCard("暂无持久化记忆块") }
                    } else {
                        items(hermesState.memoryBlocks) { (blockTitle, blockContent) ->
                            AppleCard {
                                Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                    Text(blockTitle, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                                    Text(blockContent, color = palette.secondaryText, fontSize = 12.sp)
                                }
                            }
                        }
                    }
                }

                HermesStudioTab.SKILLS_HUB -> {
                    if (hermesState.skills.isEmpty()) {
                        item { HonestEmptyCard("暂无已安装的 Hermes 技能") }
                    } else {
                        items(hermesState.skills, key = { it.id }) { skill ->
                            AppleCard {
                                Row(
                                    modifier = Modifier.padding(12.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        Text(skill.name, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                        Text(skill.description, color = palette.secondaryText, fontSize = 12.sp)
                                    }
                                    Switch(checked = skill.enabled, onCheckedChange = { onToggleSkill(skill.id) })
                                }
                            }
                        }
                    }
                }

                HermesStudioTab.TOOLSETS_TERMINAL -> {
                    item {
                        AppleCard {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text("Hermes 斜杠指令控制台", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                    listOf("/help", "/status", "/sessions").forEach { cmd ->
                                        Surface(
                                            shape = RoundedCornerShape(8.dp),
                                            color = palette.inputBg,
                                            modifier = Modifier
                                                .clip(RoundedCornerShape(8.dp))
                                                .clickable { onExecuteTerminal(cmd) },
                                        ) {
                                            Text(
                                                text = cmd,
                                                color = palette.primaryAccent,
                                                fontFamily = FontFamily.Monospace,
                                                fontSize = 12.sp,
                                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                            )
                                        }
                                    }
                                }
                                if (hermesState.terminalLines.isEmpty()) {
                                    Text("尚无控制台输出，请执行真实斜杠指令。", color = palette.secondaryText, fontSize = 12.sp)
                                } else {
                                    Surface(
                                        shape = RoundedCornerShape(10.dp),
                                        color = Color(0xFF14181E),
                                        modifier = Modifier.fillMaxWidth(),
                                    ) {
                                        Column(modifier = Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                                            hermesState.terminalLines.takeLast(20).forEach { line ->
                                                Text(line, color = Color(0xFFE5EAF0), fontFamily = FontFamily.Monospace, fontSize = 11.sp)
                                            }
                                        }
                                    }
                                }
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                    OutlinedTextField(
                                        value = terminalInput,
                                        onValueChange = { terminalInput = it },
                                        placeholder = { Text("/help 或 /status", color = palette.hintText) },
                                        singleLine = true,
                                        colors = inputColors,
                                        modifier = Modifier.weight(1f),
                                    )
                                    Button(
                                        enabled = terminalInput.isNotBlank(),
                                        onClick = {
                                            onExecuteTerminal(terminalInput.trim())
                                            terminalInput = ""
                                        },
                                        colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                    ) {
                                        Text("执行")
                                    }
                                }
                            }
                        }
                    }
                }

                HermesStudioTab.CRON_HEARTBEAT -> {
                    item {
                        AppleCard {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("定时任务 (Cron)", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                OutlinedTextField(
                                    value = cronTitle,
                                    onValueChange = { cronTitle = it },
                                    label = { Text("任务名称") },
                                    singleLine = true,
                                    colors = inputColors,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                                OutlinedTextField(
                                    value = cronExpr,
                                    onValueChange = { cronExpr = it },
                                    label = { Text("Cron 表达式") },
                                    placeholder = { Text("0 8 * * *", color = palette.hintText) },
                                    singleLine = true,
                                    colors = inputColors,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                                OutlinedTextField(
                                    value = cronPrompt,
                                    onValueChange = { cronPrompt = it },
                                    label = { Text("执行指令") },
                                    singleLine = true,
                                    colors = inputColors,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                                Button(
                                    enabled = cronTitle.isNotBlank() && cronExpr.isNotBlank() && cronPrompt.isNotBlank(),
                                    onClick = {
                                        onAddCron(cronTitle.trim(), cronExpr.trim(), cronPrompt.trim())
                                        cronTitle = ""
                                        cronExpr = ""
                                        cronPrompt = ""
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                ) {
                                    Text("添加定时任务")
                                }
                            }
                        }
                    }
                    if (hermesState.cronJobs.isEmpty()) {
                        item { HonestEmptyCard("暂无定时任务") }
                    } else {
                        items(hermesState.cronJobs, key = { it.id }) { job ->
                            AppleCard {
                                Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                                    Text(job.title, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                    Text("${job.scheduleExpr} · ${job.prompt}", color = palette.secondaryText, fontSize = 12.sp)
                                    if (job.lastRunStatus.isNotBlank()) {
                                        Text("上次运行：${job.lastRunStatus}", color = palette.primaryAccent, fontSize = 11.sp)
                                    }
                                }
                            }
                        }
                    }
                }

                HermesStudioTab.KANBAN_APPROVALS -> {
                    if (hermesState.approvals.isEmpty() && hermesState.kanbanCards.isEmpty()) {
                        item { HonestEmptyCard("暂无待审批项或任务看板卡片") }
                    } else {
                        itemsIndexed(hermesState.approvals, key = { index, appr -> "apr_${index}_${appr.id}" }) { _, appr ->
                            AppleCard {
                                Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Text(appr.toolName, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                    Text(appr.summary, color = palette.secondaryText, fontFamily = FontFamily.Monospace, fontSize = 12.sp)
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        OutlinedButton(onClick = { onDecideApproval(appr.id, false) }) {
                                            Text(strings.decline, color = palette.destructiveRed)
                                        }
                                        Button(
                                            onClick = { onDecideApproval(appr.id, true) },
                                            colors = ButtonDefaults.buttonColors(
                                                containerColor = palette.approveActionBg,
                                                contentColor = palette.approveActionContent,
                                            ),
                                        ) {
                                            Text(strings.approve)
                                        }
                                    }
                                }
                            }
                        }
                        itemsIndexed(hermesState.kanbanCards, key = { index, card -> "card_${index}_${card.id}" }) { _, card ->
                            AppleCard {
                                Row(modifier = Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Column(Modifier.weight(1f)) {
                                        Text(card.title, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                        Text("${card.column.label} · ${card.assigneeAgent}", color = palette.secondaryText, fontSize = 12.sp)
                                    }
                                    TextButton(onClick = { onAdvanceKanban(card.id) }) {
                                        Text("推进", color = palette.topBarActionColor)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// 4. Unified 4-Segment Remote Control Hub (`远程` Tab: My Devices | Assist | Cloud | AI MCP)
// ============================================================================

@Composable
fun CloudComputerWorkbenchScreen(
    computerState: CloudComputerUiState,
    mikaSnapshot: MikaBridgeUiSnapshot = MikaBridgeUiSnapshot(),
    serverEndpoint: String = DEFAULT_DESKTOP_SERVER_URL,
    isAuthenticated: Boolean = false,
    onSelectNode: (String) -> Unit,
    onAcquireLease: (String) -> Unit,
    onEmergencyTakeover: (String) -> Unit,
    onExecuteOp: (String) -> Unit,
    onAdvanceTask: (String) -> Unit,
    onOpenMikaWorkspace: () -> Unit,
    onRefreshAll: () -> Unit = {},
    onToggleRemoteAssist: (Boolean) -> Unit = {},
    onResetAssistCode: () -> Unit = {},
    onUpdateAssistCode: (String) -> Unit = {},
    onStartRemoteAssist: (assistId: String, accessCode: String, mode: String) -> Unit = { _, _, _ -> },
    onDisconnectRemoteSession: (String) -> Unit = {},
    onDisconnectAllRemoteSessions: () -> Unit = {},
    onUpdateStreamSettings: (RemoteStreamSettingsUi) -> Unit = {},
    onUpsertCloudComputer: (displayName: String, host: String, port: Int, username: String, authMethod: String, secret: String?, verify: Boolean) -> Unit = { _, _, _, _, _, _, _ -> },
    onPatchComputerPolicy: (nodeId: String, runPolicy: String) -> Unit = { _, _ -> },
    onTestComputer: (String) -> Unit = {},
    onDeleteComputer: (String) -> Unit = {},
    onIssueMcpGrant: (String) -> Unit = {},
    onRevokeMcpGrant: (String) -> Unit = {},
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val context = LocalContext.current
    val inputColors = appleFieldColors(palette)

    var activeTab by remember { mutableIntStateOf(0) } // 0=MyDevices, 1=Assist, 2=Cloud, 3=AI MCP
    var showLiveWebRtcViewer by remember { mutableStateOf(false) }

    var localCmdDraft by remember { mutableStateOf("") }
    var customCodeDraft by remember { mutableStateOf("") }
    var peerAssistIdDraft by remember { mutableStateOf("") }
    var peerAssistCodeDraft by remember { mutableStateOf("") }
    var assistMode by remember { mutableStateOf("desktop") }

    var showAddCloudForm by remember { mutableStateOf(false) }
    var cloudName by remember { mutableStateOf("") }
    var cloudHost by remember { mutableStateOf("") }
    var cloudPort by remember { mutableStateOf("22") }
    var cloudUser by remember { mutableStateOf("root") }
    var cloudSecret by remember { mutableStateOf("") }
    var cloudCmdDraft by remember { mutableStateOf("") }
    var copiedMcpClient by remember { mutableStateOf<String?>(null) }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground)
            .testTag("telegram-remote-hub-screen"),
    ) {
        // Apple Large Title + 4-Segment Switcher
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(palette.topBarBackground)
                .statusBarsPadding()
                .padding(horizontal = 16.dp, vertical = 10.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = strings.tabRemote,
                    color = palette.topBarContent,
                    fontSize = 28.sp,
                    fontWeight = FontWeight.Bold,
                )
                TextButton(onClick = onRefreshAll) {
                    Text(
                        text = if (strings.isEnglish) "Refresh" else "刷新",
                        color = palette.topBarActionColor,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
            }
            AppleSegmentedControl(
                options = listOf(
                    strings.remoteMyDevices,
                    strings.remoteAssist,
                    strings.remoteCloudServers,
                    strings.remoteAiMcp,
                ),
                selectedIndex = activeTab,
                onSelect = { activeTab = it },
            )
        }
        TelegramHairlineDivider()

        BackHandler(enabled = showLiveWebRtcViewer) {
            showLiveWebRtcViewer = false
        }

        if (showLiveWebRtcViewer) {
            Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
                RemoteComputerSurface(
                    onClose = { showLiveWebRtcViewer = false },
                    embedded = true,
                )
            }
        } else {
            LazyColumn(
                modifier = Modifier.weight(1f),
                contentPadding = PaddingValues(16.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
            when (activeTab) {
                // Segment 0: My Devices & Real Local Android Mika
                0 -> {
                    item {
                        AppleCard {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                        Box(
                                            modifier = Modifier
                                                .size(38.dp)
                                                .clip(RoundedCornerShape(10.dp))
                                                .background(if (mikaSnapshot.available) palette.secretGreen.copy(alpha = 0.16f) else palette.inputBg),
                                            contentAlignment = Alignment.Center,
                                        ) {
                                            TelegramIcon(
                                                glyph = TelegramGlyph.MIKA_PHONE,
                                                tint = if (mikaSnapshot.available) palette.secretGreen else palette.secondaryText,
                                                size = 20.dp,
                                            )
                                        }
                                        Column {
                                            Text("本机 Android · Mika 引擎", color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                                            Text(
                                                text = if (mikaSnapshot.available) "Mika 在线 · 沙盒${if (mikaSnapshot.sandboxReady) "就绪" else "未就绪"}" else "Mika 服务未连接",
                                                color = if (mikaSnapshot.available) palette.statusGreenText else palette.secondaryText,
                                                fontSize = 12.sp,
                                            )
                                        }
                                    }
                                    Button(
                                        onClick = onOpenMikaWorkspace,
                                        colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                        shape = RoundedCornerShape(10.dp),
                                    ) {
                                        Text("打开工作台", fontSize = 12.sp)
                                    }
                                }

                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                                ) {
                                    StatusBadgePill("无障碍", mikaSnapshot.accessibilityEnabled, Modifier.weight(1f))
                                    StatusBadgePill("无线 ADB", mikaSnapshot.wirelessAdbPaired, Modifier.weight(1f))
                                    StatusBadgePill("LAN MCP", mikaSnapshot.mcpLanEnabled, Modifier.weight(1f))
                                    StatusBadgePill("Relay", mikaSnapshot.mcpRelayEnabled, Modifier.weight(1f))
                                }

                                if (mikaSnapshot.mcpLanEnabled) {
                                    Text(
                                        text = "MCP LAN 端口：${mikaSnapshot.mcpLanPort} · 状态：${mikaSnapshot.mcpServerState}",
                                        color = palette.secondaryText,
                                        fontFamily = FontFamily.Monospace,
                                        fontSize = 11.sp,
                                    )
                                }

                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                    OutlinedTextField(
                                        value = localCmdDraft,
                                        onValueChange = { localCmdDraft = it },
                                        placeholder = { Text("在本机 Mika 沙盒执行命令…", color = palette.hintText) },
                                        singleLine = true,
                                        colors = inputColors,
                                        modifier = Modifier.weight(1f),
                                    )
                                    Button(
                                        enabled = localCmdDraft.isNotBlank(),
                                        onClick = {
                                            onSelectNode("local-android-mika")
                                            onExecuteOp(localCmdDraft.trim())
                                        },
                                        colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                    ) {
                                        Text("执行")
                                    }
                                }
                                if (computerState.lastOpOutput.isNotBlank()) {
                                    Surface(
                                        shape = RoundedCornerShape(10.dp),
                                        color = Color(0xFF14181E),
                                        modifier = Modifier.fillMaxWidth(),
                                    ) {
                                        Text(
                                            text = computerState.lastOpOutput,
                                            color = Color(0xFFE5EAF0),
                                            fontFamily = FontFamily.Monospace,
                                            fontSize = 11.sp,
                                            modifier = Modifier.padding(10.dp),
                                        )
                                    }
                                }
                            }
                        }
                    }

                    val localDevices = computerState.nodes.filter { it.kind == ComputerNodeKind.LOCAL_DEVICE }
                    if (localDevices.isEmpty()) {
                        item {
                            HonestEmptyCard(
                                if (isAuthenticated) "暂无其他已注册的个人设备" else "登录账号后同步桌面端与其他设备",
                            )
                        }
                    } else {
                        items(localDevices, key = { it.id }) { node ->
                            val selected = node.id == computerState.selectedNodeId
                            AppleCard {
                                Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Row(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .clickable { onSelectNode(node.id) },
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        Box(
                                            modifier = Modifier
                                                .size(10.dp)
                                                .clip(CircleShape)
                                                .background(if (node.online) palette.secretGreen else palette.unreadBadgeMuted),
                                        )
                                        Spacer(Modifier.width(10.dp))
                                        Column(Modifier.weight(1f)) {
                                            Text(node.name, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                            Text(
                                                listOfNotNull(
                                                    node.osInfo.takeIf { it.isNotBlank() },
                                                    node.resolution.takeIf { it.isNotBlank() },
                                                    node.runPolicy.takeIf { it.isNotBlank() },
                                                ).joinToString(" · "),
                                                color = palette.secondaryText,
                                                fontSize = 11.sp,
                                            )
                                        }
                                        if (selected) {
                                            Text("已选中", color = palette.primaryAccent, fontSize = 11.sp, fontWeight = FontWeight.SemiBold)
                                        }
                                    }
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        OutlinedButton(onClick = { onAcquireLease(node.id) }) {
                                            Text("申请控制权", color = palette.primaryAccent, fontSize = 12.sp)
                                        }
                                        OutlinedButton(
                                            onClick = { onEmergencyTakeover(node.id) },
                                            modifier = Modifier.testTag("computer-emergency-takeover"),
                                        ) {
                                            Text("人工接管", color = palette.destructiveRed, fontSize = 12.sp)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Segment 1: Remote Assist + Active Sessions + Live WebRTC Viewer
                1 -> {
                    item {
                        AppleCard(modifier = Modifier.testTag("cloud-computer-uu-remote-card")) {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Row(
                                    modifier = Modifier.fillMaxWidth(),
                                    horizontalArrangement = Arrangement.SpaceBetween,
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column {
                                        Text("本机远程协助 ID", color = palette.secondaryText, fontSize = 12.sp)
                                        Text(
                                            text = computerState.assistConfig.assistId.ifBlank {
                                                if (isAuthenticated) "点击刷新获取 9 位协助 ID" else "登录后自动分配"
                                            },
                                            color = palette.primaryText,
                                            fontFamily = FontFamily.Monospace,
                                            fontSize = 20.sp,
                                            fontWeight = FontWeight.Bold,
                                        )
                                    }
                                    Switch(
                                        checked = computerState.assistConfig.allowAssist,
                                        onCheckedChange = onToggleRemoteAssist,
                                    )
                                }
                                if (computerState.assistConfig.currentCode.isNotBlank()) {
                                    Text(
                                        text = "当前访问码：${computerState.assistConfig.currentCode}",
                                        color = palette.primaryAccent,
                                        fontFamily = FontFamily.Monospace,
                                        fontSize = 13.sp,
                                    )
                                }
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    OutlinedButton(onClick = onResetAssistCode, modifier = Modifier.weight(1f)) {
                                        Text("重置临时码", color = palette.primaryAccent, fontSize = 12.sp)
                                    }
                                    Button(
                                        onClick = { showLiveWebRtcViewer = true },
                                        colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                        modifier = Modifier.weight(1f).testTag("remote-open-webrtc-viewer"),
                                    ) {
                                        Text("打开实时画面", fontSize = 12.sp)
                                    }
                                }
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                    OutlinedTextField(
                                        value = customCodeDraft,
                                        onValueChange = { customCodeDraft = it },
                                        placeholder = { Text("自定义 8-16 位字母+数字访问码", color = palette.hintText) },
                                        singleLine = true,
                                        colors = inputColors,
                                        modifier = Modifier.weight(1f),
                                    )
                                    OutlinedButton(
                                        enabled = isValidCustomAssistCode(customCodeDraft),
                                        onClick = {
                                            onUpdateAssistCode(customCodeDraft.trim())
                                            customCodeDraft = ""
                                        },
                                    ) {
                                        Text(strings.save, color = palette.primaryAccent)
                                    }
                                }
                            }
                        }
                    }

                    // Connect to Peer Assist
                    item {
                        AppleCard {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text("连接到伙伴设备", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                                OutlinedTextField(
                                    value = peerAssistIdDraft,
                                    onValueChange = { peerAssistIdDraft = it },
                                    label = { Text("对方 9 位协助 ID") },
                                    singleLine = true,
                                    colors = inputColors,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                                OutlinedTextField(
                                    value = peerAssistCodeDraft,
                                    onValueChange = { peerAssistCodeDraft = it },
                                    label = { Text("对方访问码") },
                                    singleLine = true,
                                    colors = inputColors,
                                    modifier = Modifier.fillMaxWidth(),
                                )
                                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                    listOf("desktop" to "桌面", "terminal" to "终端", "file_transfer" to "文件", "ai_control" to "AI 控制").forEach { (m, lbl) ->
                                        val sel = assistMode == m
                                        Surface(
                                            shape = RoundedCornerShape(8.dp),
                                            color = if (sel) palette.selectedPillBackground else palette.inputBg,
                                            modifier = Modifier.clip(RoundedCornerShape(8.dp)).clickable { assistMode = m },
                                        ) {
                                            Text(
                                                text = lbl,
                                                color = if (sel) palette.selectedPillContent else palette.primaryText,
                                                fontSize = 11.sp,
                                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                            )
                                        }
                                    }
                                }
                                Button(
                                    enabled = peerAssistIdDraft.isNotBlank() && peerAssistCodeDraft.isNotBlank(),
                                    onClick = {
                                        onStartRemoteAssist(peerAssistIdDraft.trim(), peerAssistCodeDraft.trim(), assistMode)
                                    },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                    shape = RoundedCornerShape(10.dp),
                                    modifier = Modifier.fillMaxWidth(),
                                ) {
                                    Text("发起远程连接")
                                }
                            }
                        }
                    }

                    // Active Remote Sessions & Kill Switch
                    item {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = "活跃远程会话 (${computerState.remoteSessions.size})",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                            if (computerState.remoteSessions.isNotEmpty()) {
                                TextButton(onClick = onDisconnectAllRemoteSessions) {
                                    Text("急停断开全部", color = palette.destructiveRed, fontWeight = FontWeight.SemiBold)
                                }
                            }
                        }
                    }

                    if (computerState.remoteSessions.isEmpty()) {
                        item { HonestEmptyCard("当前没有活跃的远程控制会话") }
                    } else {
                        items(computerState.remoteSessions, key = { it.sessionId }) { sess ->
                            AppleCard {
                                Row(
                                    modifier = Modifier.padding(12.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    Column(Modifier.weight(1f)) {
                                        Text(sess.controllerName.ifBlank { sess.controllerKind }, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                        Text(
                                            buildString {
                                                append("${sess.relayMode} · ${sess.targetNodeId}")
                                                sess.rttMs?.let { append(" · ${it}ms") }
                                                sess.fps?.let { append(" · ${it}fps") }
                                            },
                                            color = palette.secondaryText,
                                            fontSize = 11.sp,
                                        )
                                    }
                                    TextButton(onClick = { onDisconnectRemoteSession(sess.sessionId) }) {
                                        Text("断开", color = palette.destructiveRed)
                                    }
                                }
                            }
                        }
                    }
                }

                // Segment 2: Cloud Servers (/api/v1/computers)
                2 -> {
                    item {
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text("云服务器节点", color = palette.secondaryText, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                            TextButton(onClick = { showAddCloudForm = !showAddCloudForm }) {
                                Text(if (showAddCloudForm) strings.done else "添加云服务器", color = palette.topBarActionColor)
                            }
                        }
                    }

                    if (showAddCloudForm) {
                        item {
                            AppleCard {
                                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    OutlinedTextField(
                                        value = cloudName,
                                        onValueChange = { cloudName = it },
                                        label = { Text("服务器名称") },
                                        singleLine = true,
                                        colors = inputColors,
                                        modifier = Modifier.fillMaxWidth(),
                                    )
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        OutlinedTextField(
                                            value = cloudHost,
                                            onValueChange = { cloudHost = it },
                                            label = { Text("主机 / IP") },
                                            singleLine = true,
                                            colors = inputColors,
                                            modifier = Modifier.weight(2f),
                                        )
                                        OutlinedTextField(
                                            value = cloudPort,
                                            onValueChange = { cloudPort = it },
                                            label = { Text("端口") },
                                            singleLine = true,
                                            colors = inputColors,
                                            modifier = Modifier.weight(1f),
                                        )
                                    }
                                    OutlinedTextField(
                                        value = cloudUser,
                                        onValueChange = { cloudUser = it },
                                        label = { Text("SSH 用户名") },
                                        singleLine = true,
                                        colors = inputColors,
                                        modifier = Modifier.fillMaxWidth(),
                                    )
                                    OutlinedTextField(
                                        value = cloudSecret,
                                        onValueChange = { cloudSecret = it },
                                        label = { Text("密码 / 私钥（可选）") },
                                        singleLine = true,
                                        colors = inputColors,
                                        modifier = Modifier.fillMaxWidth(),
                                    )
                                    Button(
                                        enabled = cloudName.isNotBlank() && cloudHost.isNotBlank(),
                                        onClick = {
                                            onUpsertCloudComputer(
                                                cloudName.trim(),
                                                cloudHost.trim(),
                                                cloudPort.toIntOrNull() ?: 22,
                                                cloudUser.trim().ifBlank { "root" },
                                                "password",
                                                cloudSecret.takeIf { it.isNotBlank() },
                                                false,
                                            )
                                            cloudName = ""
                                            cloudHost = ""
                                            cloudSecret = ""
                                            showAddCloudForm = false
                                        },
                                        colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                        modifier = Modifier.fillMaxWidth(),
                                    ) {
                                        Text("保存云服务器")
                                    }
                                }
                            }
                        }
                    }

                    val cloudNodes = computerState.nodes.filter { it.kind != ComputerNodeKind.LOCAL_DEVICE }
                    if (cloudNodes.isEmpty()) {
                        item {
                            HonestEmptyCard(
                                if (isAuthenticated) "尚未添加云服务器，点击右上角「添加云服务器」配置真实主机。" else "登录账号后管理云服务器节点。",
                            )
                        }
                    } else {
                        items(cloudNodes, key = { it.id }) { node ->
                            AppleCard {
                                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Box(
                                            modifier = Modifier
                                                .size(10.dp)
                                                .clip(CircleShape)
                                                .background(if (node.online) palette.secretGreen else palette.unreadBadgeMuted),
                                        )
                                        Spacer(Modifier.width(8.dp))
                                        Column(Modifier.weight(1f)) {
                                            Text(node.name, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                                            Text(
                                                listOfNotNull(
                                                    node.host?.takeIf { it.isNotBlank() }?.let { host ->
                                                        val port = node.sshPort
                                                        if (port != null) "$host:$port" else host
                                                    },
                                                    node.osInfo.takeIf { it.isNotBlank() },
                                                    node.runPolicy.takeIf { it.isNotBlank() },
                                                ).joinToString(" · "),
                                                color = palette.secondaryText,
                                                fontSize = 11.sp,
                                            )
                                        }
                                    }
                                    Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                        listOf("ask_every_time" to "每次询问", "allow_read_only" to "只读", "always_allow_authorized" to "自动授权").forEach { (pol, lbl) ->
                                            val sel = node.runPolicy == pol
                                            Surface(
                                                shape = RoundedCornerShape(8.dp),
                                                color = if (sel) palette.selectedPillBackground else palette.inputBg,
                                                modifier = Modifier
                                                    .clip(RoundedCornerShape(8.dp))
                                                    .clickable { onPatchComputerPolicy(node.id, pol) },
                                            ) {
                                                Text(
                                                    text = lbl,
                                                    color = if (sel) palette.selectedPillContent else palette.primaryText,
                                                    fontSize = 11.sp,
                                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 5.dp),
                                                )
                                            }
                                        }
                                    }
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        OutlinedButton(onClick = { onTestComputer(node.id) }) {
                                            Text("测试连接", color = palette.primaryAccent, fontSize = 12.sp)
                                        }
                                        OutlinedButton(onClick = { onSelectNode(node.id) }) {
                                            Text("选中终端", color = palette.primaryText, fontSize = 12.sp)
                                        }
                                        TextButton(onClick = { onDeleteComputer(node.id) }) {
                                            Text(strings.delete, color = palette.destructiveRed, fontSize = 12.sp)
                                        }
                                    }
                                }
                            }
                        }

                        // Cloud Exec Console (empty input, zero pre-filled "df -h && uptime")
                        item {
                            AppleCard {
                                Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Text(
                                        text = "远程命令执行 · ${computerState.selectedNodeId.ifBlank { "请先选择节点" }}",
                                        color = palette.primaryText,
                                        fontWeight = FontWeight.SemiBold,
                                        fontSize = 13.sp,
                                    )
                                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                        OutlinedTextField(
                                            value = cloudCmdDraft,
                                            onValueChange = { cloudCmdDraft = it },
                                            placeholder = { Text("输入要执行的真实命令…", color = palette.hintText) },
                                            singleLine = true,
                                            colors = inputColors,
                                            modifier = Modifier.weight(1f),
                                        )
                                        Button(
                                            enabled = cloudCmdDraft.isNotBlank() && computerState.selectedNodeId.isNotBlank(),
                                            onClick = { onExecuteOp(cloudCmdDraft.trim()) },
                                            colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                                        ) {
                                            Text("执行")
                                        }
                                    }
                                    if (computerState.lastOpOutput.isNotBlank()) {
                                        Surface(
                                            shape = RoundedCornerShape(10.dp),
                                            color = Color(0xFF14181E),
                                            modifier = Modifier.fillMaxWidth(),
                                        ) {
                                            Text(
                                                text = computerState.lastOpOutput,
                                                color = Color(0xFFE5EAF0),
                                                fontFamily = FontFamily.Monospace,
                                                fontSize = 11.sp,
                                                modifier = Modifier.padding(10.dp),
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Segment 3: AI MCP Hub (Real OAuth 2.1 Grants + 1-click Copyable Client Snippets)
                else -> {
                    item {
                        AppleCard(modifier = Modifier.testTag("cloud-computer-mcp-grants-card")) {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text("已授权的 AI MCP 客户端 (OAuth 2.1)", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                                Text(
                                    text = "MCP 令牌由外部 AI 客户端通过标准 OAuth 2.1 流程向服务器申请，本页仅展示真实活跃授权并支持一键吊销。",
                                    color = palette.secondaryText,
                                    fontSize = 12.sp,
                                )
                                if (computerState.mcpGrants.isEmpty()) {
                                    Text("暂无活跃 MCP 授权会话。", color = palette.secondaryText, fontSize = 13.sp)
                                } else {
                                    computerState.mcpGrants.forEach { grant ->
                                        Row(
                                            modifier = Modifier
                                                .fillMaxWidth()
                                                .clip(RoundedCornerShape(10.dp))
                                                .background(palette.cardBg)
                                                .padding(10.dp),
                                            verticalAlignment = Alignment.CenterVertically,
                                        ) {
                                            Column(Modifier.weight(1f)) {
                                                Text(grant.clientName, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                                Text(grant.scopes.joinToString(", "), color = palette.secondaryText, fontSize = 11.sp)
                                            }
                                            TextButton(onClick = { onRevokeMcpGrant(grant.familyId) }) {
                                                Text("吊销", color = palette.destructiveRed)
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 4 Copyable Client Config Snippets (Antigravity, Claude, Cursor, VS Code)
                    item {
                        val mcpUrl = "${serverEndpoint.trimEnd('/')}/mcp"
                        val snippets = listOf(
                            "Antigravity" to """{"mcpServers":{"fabushi":{"url":"$mcpUrl"}}}""",
                            "Claude" to """{"mcpServers":{"fabushi":{"type":"http","url":"$mcpUrl"}}}""",
                            "Cursor" to """{"mcpServers":{"fabushi":{"url":"$mcpUrl"}}}""",
                            "VS Code" to """{"servers":{"fabushi":{"type":"http","url":"$mcpUrl"}}}""",
                        )
                        AppleCard {
                            Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                Text("客户端一键接入配置", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                                snippets.forEach { (clientName, configJson) ->
                                    Surface(
                                        shape = RoundedCornerShape(10.dp),
                                        color = palette.cardBg,
                                        modifier = Modifier.fillMaxWidth(),
                                    ) {
                                        Row(
                                            modifier = Modifier.padding(10.dp),
                                            verticalAlignment = Alignment.CenterVertically,
                                        ) {
                                            Column(Modifier.weight(1f)) {
                                                Text(clientName, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                                                Text(
                                                    text = configJson,
                                                    color = palette.secondaryText,
                                                    fontFamily = FontFamily.Monospace,
                                                    fontSize = 10.sp,
                                                    maxLines = 2,
                                                    overflow = TextOverflow.Ellipsis,
                                                )
                                            }
                                            TextButton(
                                                onClick = {
                                                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                                                    cm?.setPrimaryClip(ClipData.newPlainText("mcp_$clientName", configJson))
                                                    copiedMcpClient = clientName
                                                },
                                            ) {
                                                Text(
                                                    text = if (copiedMcpClient == clientName) strings.copied else strings.copy,
                                                    color = palette.topBarActionColor,
                                                    fontSize = 12.sp,
                                                )
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        }
    }
}

// ============================================================================
// Shared Apple Primitives
// ============================================================================

@Composable
internal fun AppleSegmentedControl(
    options: List<String>,
    selectedIndex: Int,
    onSelect: (Int) -> Unit,
    modifier: Modifier = Modifier,
) {
    val palette = LocalTelegramPalette.current
    Surface(
        shape = RoundedCornerShape(10.dp),
        color = palette.inputBg,
        modifier = modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(3.dp),
            horizontalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            options.forEachIndexed { idx, label ->
                val selected = idx == selectedIndex
                Surface(
                    shape = RoundedCornerShape(8.dp),
                    color = if (selected) palette.modalBg else Color.Transparent,
                    shadowElevation = if (selected && !palette.isDark) 1.dp else 0.dp,
                    modifier = Modifier
                        .weight(1f)
                        .clip(RoundedCornerShape(8.dp))
                        .clickable { onSelect(idx) },
                ) {
                    Text(
                        text = label,
                        color = if (selected) palette.primaryText else palette.secondaryText,
                        fontSize = 12.sp,
                        fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                        textAlign = TextAlign.Center,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.padding(vertical = 7.dp, horizontal = 4.dp),
                    )
                }
            }
        }
    }
}

@Composable
private fun AppleCard(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Surface(
        shape = RoundedCornerShape(14.dp),
        color = palette.modalBg,
        modifier = modifier
            .fillMaxWidth()
            .border(0.5.dp, palette.borderColor, RoundedCornerShape(14.dp)),
    ) {
        content()
    }
}

@Composable
private fun HonestEmptyCard(message: String) {
    val palette = LocalTelegramPalette.current
    AppleCard {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(22.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(message, color = palette.secondaryText, fontSize = 13.sp, textAlign = TextAlign.Center)
        }
    }
}

@Composable
private fun StatusBadgePill(label: String, active: Boolean, modifier: Modifier = Modifier) {
    val palette = LocalTelegramPalette.current
    Surface(
        shape = RoundedCornerShape(8.dp),
        color = if (active) palette.secretGreen.copy(alpha = 0.14f) else palette.inputBg,
        modifier = modifier,
    ) {
        Text(
            text = label,
            color = if (active) palette.statusGreenText else palette.secondaryText,
            fontSize = 10.sp,
            fontWeight = FontWeight.SemiBold,
            textAlign = TextAlign.Center,
            maxLines = 1,
            modifier = Modifier.padding(vertical = 5.dp, horizontal = 4.dp),
        )
    }
}

@Composable
private fun appleFieldColors(palette: TelegramPalette) = OutlinedTextFieldDefaults.colors(
    focusedTextColor = palette.primaryText,
    unfocusedTextColor = palette.primaryText,
    cursorColor = palette.searchCursorColor,
    focusedBorderColor = palette.primaryAccent,
    unfocusedBorderColor = palette.borderColor,
)

@Composable
internal fun WorkspaceTopBar(
    title: String,
    subtitle: String,
    glyph: TelegramGlyph,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(palette.topBarBackground),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .statusBarsPadding()
                .padding(horizontal = 10.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier
                    .size(34.dp)
                    .clip(CircleShape)
                    .clickable(onClick = onBack),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.CHEVRON_LEFT, tint = palette.topBarActionColor, size = 20.dp)
            }
            Spacer(Modifier.width(6.dp))
            TelegramIcon(glyph, tint = palette.primaryAccent, size = 18.dp)
            Spacer(Modifier.width(8.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = title,
                    color = palette.topBarContent,
                    fontWeight = FontWeight.Bold,
                    fontSize = 16.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (subtitle.isNotBlank()) {
                    Text(
                        text = subtitle,
                        color = palette.secondaryText,
                        fontSize = 11.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
        TelegramHairlineDivider()
    }
}

@Composable
private fun GatewayMetric(label: String, value: String) {
    val palette = LocalTelegramPalette.current
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Text(value, color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 15.sp)
        Text(label, color = palette.secondaryText, fontSize = 11.sp)
    }
}

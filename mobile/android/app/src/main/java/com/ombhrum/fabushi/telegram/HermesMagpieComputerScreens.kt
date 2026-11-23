package com.ombhrum.fabushi.telegram

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
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

// ============================================================================
// Mobile-Native Bots & Mika Hub (`智能体` Tab — Desktop v1.6.9 Aligned, Zero Desktop-Only Bloat)
// ============================================================================

@Composable
fun BotFatherWorkspaceScreen(
    existingBots: List<TelegramChatItem>,
    fabushiBots: List<FabushiBotItem> = emptyList(),
    mikaSnapshot: MikaBridgeUiSnapshot = MikaBridgeUiSnapshot(),
    lastCreatedBotToken: String? = null,
    isAuthenticated: Boolean = false,
    onOpenBot: (FabushiBotItem) -> Unit,
    onOpenBotChat: (String) -> Unit,
    onOpenBotByUsername: (username: String, displayName: String, description: String) -> Unit,
    onOpenMikaWorkspace: () -> Unit,
    onCreateBot: (name: String, description: String, handleHint: String) -> Unit,
    onBotFatherNlCreate: (prompt: String) -> Unit = {},
    onRefreshBots: () -> Unit = {},
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val context = LocalContext.current
    val inputColors = appleFieldColors(palette)

    var nlPrompt by remember { mutableStateOf("") }
    var botName by remember { mutableStateOf("") }
    var botHandle by remember { mutableStateOf("") }
    var botDescription by remember { mutableStateOf("") }
    var showCreateForm by remember { mutableStateOf(false) }

    // Deduplicate chat-only bots that are not already represented in `fabushiBots`
    val extraBotChats = remember(existingBots, fabushiBots) {
        existingBots.filter { ch ->
            fabushiBots.none { b ->
                (b.companionChatId != null && ch.id == b.companionChatId.toString()) ||
                    (b.botUserId > 0L && ch.peerUserId == b.botUserId) ||
                    matchesBotUsername(ch.username, b.username)
            }
        }
    }

    val totalBotsCount = fabushiBots.size + extraBotChats.size

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground)
            .testTag("telegram-botfather-screen"),
    ) {
        // Desktop v1.6.9 / Apple Large Title Header (clean, zero desktop workbench tabs)
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(palette.topBarBackground)
                .statusBarsPadding()
                .padding(horizontal = 16.dp, vertical = 10.dp),
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
                TextButton(onClick = onRefreshBots) {
                    Text(
                        if (strings.isEnglish) "Refresh" else "刷新",
                        color = palette.topBarActionColor,
                        fontWeight = FontWeight.SemiBold,
                    )
                }
            }
        }
        TelegramHairlineDivider()

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            // 1. Android-Native Mika Phone Agent Card (mobile-native capability preserved)
            item {
                val mikaReady = mikaSnapshot.accessibilityEnabled || mikaSnapshot.wirelessAdbPaired || mikaSnapshot.available
                val mikaStatusLabel = when {
                    mikaSnapshot.activeTask != null ->
                        if (strings.isEnglish) "Running: ${mikaSnapshot.activeTask.prompt}" else "执行中：${mikaSnapshot.activeTask.prompt}"
                    mikaSnapshot.accessibilityEnabled ->
                        if (strings.isEnglish) "Accessibility Ready · Tap to control phone" else "无障碍已就绪 · 点击进入控机工作台"
                    mikaSnapshot.wirelessAdbPaired ->
                        if (strings.isEnglish) "Wireless ADB Ready · Tap to open" else "无线 ADB 已配对 · 点击进入工作台"
                    else -> strings.mikaCardSubtitle
                }
                AppleCard(
                    modifier = Modifier
                        .clip(RoundedCornerShape(14.dp))
                        .clickable(onClick = onOpenMikaWorkspace)
                        .testTag("bots-mika-card"),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(14.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(
                            modifier = Modifier
                                .size(46.dp)
                                .clip(CircleShape)
                                .background(palette.primaryAccent.copy(alpha = 0.14f)),
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.MIKA_PHONE, tint = palette.primaryAccent, size = 24.dp)
                        }
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                Text(
                                    text = strings.mikaCardTitle,
                                    color = palette.primaryText,
                                    fontWeight = FontWeight.SemiBold,
                                    fontSize = 15.sp,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Surface(
                                    shape = RoundedCornerShape(6.dp),
                                    color = if (mikaReady) palette.secretGreen.copy(alpha = 0.14f) else palette.inputBg,
                                ) {
                                    Text(
                                        text = if (mikaReady) {
                                            if (strings.isEnglish) "Ready" else "就绪"
                                        } else {
                                            if (strings.isEnglish) "On-Device" else "本机"
                                        },
                                        color = if (mikaReady) palette.statusGreenText else palette.secondaryText,
                                        fontSize = 10.sp,
                                        fontWeight = FontWeight.SemiBold,
                                        modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                    )
                                }
                            }
                            Spacer(Modifier.height(2.dp))
                            Text(
                                text = mikaStatusLabel,
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 14.dp)
                    }
                }
            }

            // 2. Section Header: Server Bots & Official Bots
            item {
                Text(
                    text = if (strings.isEnglish) {
                        "Bots ($totalBotsCount)"
                    } else {
                        "机器人与智能体 ($totalBotsCount)"
                    },
                    color = palette.secondaryText,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }

            // Always-available @BotFather quick entry (works immediately on tap)
            item {
                AppleCard(
                    modifier = Modifier
                        .clip(RoundedCornerShape(14.dp))
                        .clickable {
                            onOpenBotByUsername(
                                "BotFather",
                                "BotFather",
                                if (strings.isEnglish) "Create and manage your Fabushi bots and Mini Apps."
                                else "创建并管理您的 Fabushi 智能体与小程序。",
                            )
                        }
                        .testTag("bot-row-botfather"),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TelegramAvatar(
                            idSeed = "botfather",
                            title = "BotFather",
                            size = 44.dp,
                            avatarShape = "crown",
                            avatarColor = "blue",
                            isBot = true,
                        )
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(
                                text = "BotFather",
                                color = palette.primaryText,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp,
                            )
                            Text(
                                text = if (strings.isEnglish) "@BotFather · Create & manage bots" else "@BotFather · 官方机器人创建与管理",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 14.dp)
                    }
                }
            }

            if (fabushiBots.isEmpty() && extraBotChats.isEmpty()) {
                item {
                    AppleCard {
                        Column(
                            modifier = Modifier.padding(18.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            Text(
                                text = if (isAuthenticated) {
                                    if (strings.isEnglish) "No custom bots yet" else "尚未创建自定义智能体"
                                } else {
                                    if (strings.isEnglish) "Sign in to sync your bots" else "登录账号后自动同步云端智能体"
                                },
                                color = palette.primaryText,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp,
                            )
                            Text(
                                text = if (strings.isEnglish) {
                                    "Tap @BotFather above or describe a bot below to create one instantly."
                                } else {
                                    "点击上方 @BotFather 或在下方一句话描述，即可立刻创建并开始对话。"
                                },
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                            )
                        }
                    }
                }
            }

            // Real Server Bots (`fabushiBots`) — EVERY click is guaranteed responsive via `onOpenBot(bot)`!
            items(fabushiBots, key = { "fbot_${it.botUserId}_${it.agentId}_${it.username}" }) { bot ->
                val activityText = formatBotActivitySubtitle(
                    bot.executionState,
                    null,
                    if (strings.isEnglish) "en-US" else "zh-CN",
                )
                val isBusy = activityText != null
                val hue = resolveBotHueColors(
                    bot.avatarColor.ifBlank { botHueNameForId(bot.botUserId) },
                    palette.isDark,
                )
                AppleCard(
                    modifier = Modifier
                        .clip(RoundedCornerShape(14.dp))
                        .clickable { onOpenBot(bot) }
                        .testTag("bot-row-${bot.username.trimStart('@').lowercase()}"),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
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
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                Text(
                                    text = bot.displayName.ifBlank { bot.username },
                                    color = palette.primaryText,
                                    fontWeight = FontWeight.SemiBold,
                                    fontSize = 15.sp,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.weight(1f, fill = false),
                                )
                                if (bot.preferredModel.isNotBlank()) {
                                    Surface(
                                        shape = RoundedCornerShape(6.dp),
                                        color = hue.pillBg,
                                    ) {
                                        Text(
                                            text = bot.preferredModel.substringAfterLast('/'),
                                            color = hue.pillText,
                                            fontSize = 10.sp,
                                            fontWeight = FontWeight.Medium,
                                            maxLines = 1,
                                            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                                        )
                                    }
                                }
                            }
                            val handle = bot.username.takeIf { it.isNotBlank() }
                                ?.let { if (it.startsWith("@")) it else "@$it" }
                                .orEmpty()
                            val subtitle = activityText
                                ?: bot.description.takeIf { it.isNotBlank() }
                                ?: handle.ifBlank { if (strings.isEnglish) "Tap to chat" else "轻触开始对话" }
                            Text(
                                text = subtitle,
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

            // Extra Bot Chats already present in conversation list
            items(extraBotChats, key = { "cbot_${it.id}" }) { bot ->
                val activityText = formatBotActivitySubtitle(
                    bot.botExecutionState,
                    bot.botActivityDetail,
                    if (strings.isEnglish) "en-US" else "zh-CN",
                )
                AppleCard(
                    modifier = Modifier
                        .clip(RoundedCornerShape(14.dp))
                        .clickable { onOpenBotChat(bot.id) }
                        .testTag("bot-chat-row-${bot.id}"),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
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
                            Text(
                                text = bot.title,
                                color = palette.primaryText,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                            Text(
                                text = activityText ?: bot.subtitle.ifBlank { bot.preview },
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

            // Last Created Bot Token Banner (with Copy button)
            if (!lastCreatedBotToken.isNullOrBlank()) {
                item {
                    AppleCard {
                        Column(
                            modifier = Modifier.padding(14.dp),
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.SpaceBetween,
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    text = if (strings.isEnglish) "New Bot API Token" else "新创建的 Bot API Token",
                                    color = palette.statusGreenText,
                                    fontWeight = FontWeight.SemiBold,
                                    fontSize = 13.sp,
                                )
                                TextButton(onClick = {
                                    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                                    cm?.setPrimaryClip(ClipData.newPlainText("bot_token", lastCreatedBotToken))
                                }) {
                                    Text(strings.copy, color = palette.primaryAccent, fontSize = 12.sp)
                                }
                            }
                            Text(
                                text = lastCreatedBotToken,
                                color = palette.primaryText,
                                fontFamily = FontFamily.Monospace,
                                fontSize = 11.sp,
                            )
                        }
                    }
                }
            }

            // 3. Natural Language Quick Bot Creation (matching Desktop BotFather)
            item {
                AppleCard {
                    Column(
                        modifier = Modifier.padding(14.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Text(
                            text = if (strings.isEnglish) "Quick Create with BotFather" else "一句话创建智能体",
                            color = palette.primaryText,
                            fontWeight = FontWeight.SemiBold,
                            fontSize = 15.sp,
                        )
                        OutlinedTextField(
                            value = nlPrompt,
                            onValueChange = { nlPrompt = it },
                            label = {
                                Text(
                                    if (strings.isEnglish) {
                                        "Describe what your bot should do..."
                                    } else {
                                        "描述你想要的智能体（例：帮我每天整理日程与摘要）"
                                    },
                                )
                            },
                            shape = RoundedCornerShape(12.dp),
                            colors = inputColors,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Button(
                            enabled = nlPrompt.isNotBlank() && isAuthenticated,
                            onClick = {
                                onBotFatherNlCreate(nlPrompt.trim())
                                nlPrompt = ""
                            },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = palette.primaryAccent,
                                contentColor = Color.White,
                            ),
                            shape = RoundedCornerShape(12.dp),
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(
                                if (strings.isEnglish) "Create & Open Chat" else "立即创建并开始对话",
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                }
            }

            // 4. Structured Manual Bot Creation (collapsible)
            item {
                AppleCard {
                    Column(
                        modifier = Modifier.padding(14.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { showCreateForm = !showCreateForm },
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = if (strings.isEnglish) "Custom Bot Configuration" else "自定义创建智能体",
                                color = palette.primaryText,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp,
                            )
                            Text(
                                text = if (showCreateForm) strings.done else if (strings.isEnglish) "Expand" else "展开",
                                color = palette.topBarActionColor,
                                fontSize = 13.sp,
                                fontWeight = FontWeight.Medium,
                            )
                        }
                        if (showCreateForm) {
                            OutlinedTextField(
                                value = botName,
                                onValueChange = { botName = it },
                                label = { Text(if (strings.isEnglish) "Display Name" else "智能体名称") },
                                singleLine = true,
                                shape = RoundedCornerShape(12.dp),
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = botHandle,
                                onValueChange = { botHandle = it },
                                label = { Text(if (strings.isEnglish) "@username_bot (optional)" else "@用户名_bot（可选）") },
                                singleLine = true,
                                shape = RoundedCornerShape(12.dp),
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            OutlinedTextField(
                                value = botDescription,
                                onValueChange = { botDescription = it },
                                label = { Text(if (strings.isEnglish) "Description / Persona" else "简介与角色设定") },
                                shape = RoundedCornerShape(12.dp),
                                colors = inputColors,
                                modifier = Modifier.fillMaxWidth(),
                            )
                            Button(
                                enabled = botName.isNotBlank() && isAuthenticated,
                                onClick = {
                                    onCreateBot(botName.trim(), botDescription.trim(), botHandle.trim())
                                    botName = ""
                                    botHandle = ""
                                    botDescription = ""
                                    showCreateForm = false
                                },
                                colors = ButtonDefaults.buttonColors(
                                    containerColor = palette.primaryAccent,
                                    contentColor = Color.White,
                                ),
                                shape = RoundedCornerShape(12.dp),
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Text(strings.newBotAction, fontWeight = FontWeight.SemiBold)
                            }
                        }
                    }
                }
            }
        }
    }
}

// ============================================================================
// Clean Mobile AI Providers Settings Subpage (Desktop Settings parity — zero local CLI/PII debug)
// ============================================================================

@Composable
internal fun AiProvidersSettingsContent(
    gatewayState: MagpieGatewayUiState,
    onSetStrategy: (MagpieRoutingStrategy) -> Unit,
    onConfigureProvider: (providerId: String, apiKey: String) -> Unit,
    onSyncModelsDev: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val inputColors = appleFieldColors(palette)
    var selectedProviderId by remember { mutableStateOf("openai") }
    var apiKeyDraft by remember { mutableStateOf("") }
    var query by remember { mutableStateOf("") }

    val filteredPresets = remember(gatewayState.presets, query) {
        val q = query.trim().lowercase()
        val mobileProviders = gatewayState.presets.filterNot { it.region.equals("Local", ignoreCase = true) }
        if (q.isEmpty()) mobileProviders.take(18)
        else mobileProviders.filter {
            it.name.lowercase().contains(q) || it.id.lowercase().contains(q) || it.flagshipModel.lowercase().contains(q)
        }
    }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .testTag("settings-ai-providers-page"),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            AppleCard {
                Column(
                    modifier = Modifier.padding(14.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Text(
                        text = if (strings.isEnglish) "Configure Provider API Key" else "配置模型供应商 API Key",
                        color = palette.primaryText,
                        fontWeight = FontWeight.SemiBold,
                        fontSize = 15.sp,
                    )
                    Text(
                        text = if (strings.isEnglish) {
                            "Selected: $selectedProviderId · Synced securely with Mika & Bot runtime."
                        } else {
                            "当前选中：$selectedProviderId · 配置后可用于智能体与 Mika 对话。"
                        },
                        color = palette.secondaryText,
                        fontSize = 12.sp,
                    )
                    OutlinedTextField(
                        value = apiKeyDraft,
                        onValueChange = { apiKeyDraft = it },
                        label = { Text("API Key (sk-... / AIza...)") },
                        singleLine = true,
                        shape = RoundedCornerShape(12.dp),
                        colors = inputColors,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        Button(
                            enabled = apiKeyDraft.isNotBlank(),
                            onClick = {
                                onConfigureProvider(selectedProviderId, apiKeyDraft.trim())
                                apiKeyDraft = ""
                            },
                            colors = ButtonDefaults.buttonColors(
                                containerColor = palette.primaryAccent,
                                contentColor = Color.White,
                            ),
                            shape = RoundedCornerShape(10.dp),
                            modifier = Modifier.weight(1f),
                        ) {
                            Text(strings.save, fontSize = 13.sp, fontWeight = FontWeight.SemiBold)
                        }
                        OutlinedButton(
                            onClick = onSyncModelsDev,
                            shape = RoundedCornerShape(10.dp),
                        ) {
                            Text(
                                if (strings.isEnglish) "Sync Models" else "同步模型目录",
                                color = palette.primaryAccent,
                                fontSize = 12.sp,
                            )
                        }
                    }
                }
            }
        }

        item {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                label = { Text(if (strings.isEnglish) "Filter providers..." else "筛选供应商（OpenAI / Claude / Gemini / DeepSeek…）") },
                singleLine = true,
                shape = RoundedCornerShape(12.dp),
                colors = inputColors,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        items(filteredPresets, key = { it.id }) { preset ->
            val selected = preset.id == selectedProviderId
            AppleCard(
                modifier = Modifier
                    .clip(RoundedCornerShape(14.dp))
                    .clickable { selectedProviderId = preset.id },
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 14.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = preset.name,
                            color = palette.primaryText,
                            fontWeight = FontWeight.SemiBold,
                            fontSize = 14.sp,
                        )
                        Text(
                            text = "${preset.flagshipModel} · ${preset.protocol}",
                            color = palette.secondaryText,
                            fontSize = 12.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    Surface(
                        shape = RoundedCornerShape(8.dp),
                        color = when {
                            preset.configured -> palette.secretGreen.copy(alpha = 0.14f)
                            selected -> palette.primaryAccent.copy(alpha = 0.14f)
                            else -> palette.inputBg
                        },
                    ) {
                        Text(
                            text = when {
                                preset.configured -> if (strings.isEnglish) "Configured" else "已配置"
                                selected -> if (strings.isEnglish) "Selected" else "已选"
                                else -> if (strings.isEnglish) "Select" else "选择"
                            },
                            color = when {
                                preset.configured -> palette.statusGreenText
                                selected -> palette.primaryAccent
                                else -> palette.secondaryText
                            },
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                        )
                    }
                }
            }
        }
    }
}

// ============================================================================
// Shared Apple / Desktop v1.6.9 Primitives
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
        color = palette.cardBg,
        modifier = modifier
            .fillMaxWidth()
            .border(0.5.dp, palette.borderColor, RoundedCornerShape(14.dp)),
    ) {
        content()
    }
}

@Composable
private fun appleFieldColors(palette: TelegramPalette) = OutlinedTextFieldDefaults.colors(
    focusedTextColor = palette.primaryText,
    unfocusedTextColor = palette.primaryText,
    cursorColor = palette.searchCursorColor,
    focusedBorderColor = palette.primaryAccent,
    unfocusedBorderColor = palette.borderColor,
    focusedContainerColor = palette.inputBg,
    unfocusedContainerColor = palette.inputBg,
    focusedLabelColor = palette.primaryAccent,
    unfocusedLabelColor = palette.secondaryText,
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
            .background(palette.topBarBackground)
            .statusBarsPadding(),
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(54.dp)
                .padding(horizontal = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier
                    .size(40.dp)
                    .clip(CircleShape)
                    .clickable(onClick = onBack),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.CHEVRON_LEFT, tint = palette.topBarActionColor, size = 20.dp)
            }
            Spacer(Modifier.width(4.dp))
            TelegramIcon(glyph, tint = palette.primaryAccent, size = 20.dp)
            Spacer(Modifier.width(10.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = title,
                    color = palette.topBarContent,
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
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

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
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
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
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.ConversationKind
import java.util.Locale

private enum class MuseInspectorTab {
    PROFILE,
    MEMBERS_OR_AGENT,
    SHARED_MEDIA,
    SETTINGS,
}

private enum class MuseMediaFilter(val labelZh: String, val labelEn: String) {
    MEDIA("媒体", "Media"),
    FILES("文件", "Files"),
    VOICE("语音", "Voice"),
    LINKS("链接", "Links"),
    MUSIC("音频", "Music"),
}

/**
 * Desktop v1.6.6 Muse 4-Tab Peer / Bot Context Inspector rendered as a root-level Apple sheet.
 * Supports Profile, Members / Agent & Computers, Shared Media (5 sub-tabs), and Settings,
 * including inline 9-shape x 11-color Bot Avatar customizer wired to `patchFabushiBot`.
 */
@Composable
internal fun MuseInspectorSheet(
    visible: Boolean,
    chat: TelegramChatItem,
    messages: List<TelegramMessageItem>,
    fabushiBot: FabushiBotItem?,
    computers: List<ComputerNodeItem>,
    boundNodeId: String?,
    onDismiss: () -> Unit,
    onToggleMute: () -> Unit,
    onTogglePin: () -> Unit,
    onSetTtl: (Int) -> Unit,
    onClearHistory: () -> Unit,
    onLeaveChat: () -> Unit,
    onOpenSecretVerify: () -> Unit,
    onOpenChannelStats: () -> Unit,
    onOpenMediaViewer: (TelegramMessageItem) -> Unit,
    onSendQuickCommand: (String) -> Unit,
    onPatchBot: (
        botId: String,
        displayName: String?,
        description: String?,
        avatarShape: String?,
        avatarColor: String?,
        preferredModel: String?,
        approvalMode: String?,
    ) -> Unit,
    onBindBotComputer: (botId: String, nodeId: String?, clear: Boolean) -> Unit,
    onToast: (String) -> Unit,
) {
    if (!visible) return
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val context = LocalContext.current

    var activeTab by remember(chat.id) { mutableStateOf(MuseInspectorTab.PROFILE) }
    var mediaFilter by remember(chat.id) { mutableStateOf(MuseMediaFilter.MEDIA) }
    var editingAvatar by remember(chat.id) { mutableStateOf(false) }
    var editingBotConfig by remember(chat.id) { mutableStateOf(false) }

    val botIdKey = remember(fabushiBot, chat) {
        fabushiBot?.let {
            if (it.botUserId > 0L) it.botUserId.toString()
            else it.agentId.ifBlank { it.username }
        } ?: ""
    }
    var draftShape by remember(chat.id, fabushiBot?.avatarShape) {
        mutableStateOf((fabushiBot?.avatarShape ?: chat.avatarShape ?: "classic").ifBlank { "classic" })
    }
    var draftColor by remember(chat.id, fabushiBot?.avatarColor) {
        mutableStateOf((fabushiBot?.avatarColor ?: chat.avatarColor ?: "blue").ifBlank { "blue" })
    }
    var customHexInput by remember(chat.id, fabushiBot?.avatarColor) {
        val initial = (fabushiBot?.avatarColor ?: chat.avatarColor).orEmpty().trim()
        mutableStateOf(if (isValidHexColor(initial)) initial.uppercase(Locale.ROOT) else "")
    }
    var draftModel by remember(chat.id, fabushiBot?.preferredModel) {
        mutableStateOf(fabushiBot?.preferredModel.orEmpty())
    }
    var draftApproval by remember(chat.id, fabushiBot?.approvalMode) {
        mutableStateOf(fabushiBot?.approvalMode.orEmpty().ifBlank { "ask" })
    }
    var aboutExpanded by remember(chat.id) { mutableStateOf(true) }
    var quickCmdsExpanded by remember(chat.id) { mutableStateOf(true) }
    var agentConfigExpanded by remember(chat.id) { mutableStateOf(true) }
    var boundComputerExpanded by remember(chat.id) { mutableStateOf(true) }
    var participantsExpanded by remember(chat.id) { mutableStateOf(true) }

    val isBotChat = chat.isBot || fabushiBot != null
    val liveBotStatus = formatBotActivitySubtitle(
        executionState = chat.botExecutionState.ifBlank { fabushiBot?.executionState.orEmpty() },
        detail = chat.botActivityDetail,
        uiLanguage = if (strings.isEnglish) "en-US" else "zh-CN",
    )

    TelegramBottomSheet(visible = visible, onDismiss = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .heightIn(max = 620.dp)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp)
                .testTag("muse-inspector-sheet"),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            // Top bar close pill
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.SpaceBetween,
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = if (strings.isEnglish) "Details" else "详情",
                    color = palette.primaryText,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 16.sp,
                )
                Text(
                    text = strings.done,
                    color = palette.topBarActionColor,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 15.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .clickable(onClick = onDismiss)
                        .padding(horizontal = 6.dp, vertical = 4.dp),
                )
            }

            Spacer(Modifier.height(10.dp))

            // 80dp Hero Avatar with edit badge for owned Fabushi bots
            Box(contentAlignment = Alignment.BottomEnd) {
                TelegramAvatar(
                    idSeed = chat.id,
                    title = chat.title,
                    size = 80.dp,
                    kind = chat.kind,
                    online = chat.online,
                    isSecret = chat.isSecretChat,
                    isCompanionGhost = chat.isCompanionGhost && !isBotChat,
                    avatarShape = if (isBotChat) draftShape else chat.avatarShape,
                    avatarColor = if (isBotChat) draftColor else chat.avatarColor,
                    isBot = isBotChat,
                    botActivityActive = liveBotStatus != null,
                )
                if (fabushiBot != null && botIdKey.isNotBlank()) {
                    Box(
                        modifier = Modifier
                            .size(28.dp)
                            .clip(CircleShape)
                            .background(palette.primaryAccent)
                            .border(2.dp, palette.modalBg, CircleShape)
                            .clickable { editingAvatar = !editingAvatar }
                            .testTag("muse-bot-avatar-edit-button"),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.PENCIL, tint = Color.White, size = 14.dp)
                    }
                }
            }

            Spacer(Modifier.height(10.dp))

            Text(
                text = chat.title,
                color = palette.primaryText,
                fontSize = 20.sp,
                fontWeight = FontWeight.Bold,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            val handle = chat.username?.takeIf { it.isNotBlank() }?.let {
                if (it.startsWith("@")) it else "@$it"
            } ?: fabushiBot?.username?.takeIf { it.isNotBlank() }?.let {
                if (it.startsWith("@")) it else "@$it"
            }
            if (handle != null) {
                Text(
                    text = handle,
                    color = palette.primaryAccent,
                    fontSize = 13.sp,
                    modifier = Modifier
                        .clip(RoundedCornerShape(6.dp))
                        .clickable {
                            copyToClipboard(context, handle)
                            onToast(if (strings.isEnglish) "Copied $handle" else "已复制 $handle")
                        }
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
            if (liveBotStatus != null) {
                Text(
                    text = liveBotStatus,
                    color = palette.statusGreenText,
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                )
            }

            // Inline 9-shape x 11-color Bot Avatar Picker (Desktop v1.6.6)
            if (editingAvatar && fabushiBot != null && botIdKey.isNotBlank()) {
                Spacer(Modifier.height(12.dp))
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = palette.cardBg,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Column(
                        modifier = Modifier.padding(12.dp),
                        verticalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        Text(
                            text = if (strings.isEnglish) "Bot Avatar Shape (9)" else "智能体头像形态 (9 种)",
                            color = palette.secondaryText,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                        DesktopV166BotAvatarShapes.chunked(3).forEach { rowShapes ->
                            Row(
                                modifier = Modifier.fillMaxWidth(),
                                horizontalArrangement = Arrangement.spacedBy(8.dp),
                            ) {
                                rowShapes.forEach { (key, label) ->
                                    val selected = draftShape.equals(key, ignoreCase = true)
                                    Surface(
                                        shape = RoundedCornerShape(10.dp),
                                        color = if (selected) palette.selectedPillBackground else palette.inputBg,
                                        modifier = Modifier
                                            .weight(1f)
                                            .clip(RoundedCornerShape(10.dp))
                                            .clickable { draftShape = key },
                                    ) {
                                        Row(
                                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 6.dp),
                                            verticalAlignment = Alignment.CenterVertically,
                                            horizontalArrangement = Arrangement.Center,
                                        ) {
                                            BotCustomAvatar(
                                                idSeed = chat.id,
                                                shape = key,
                                                colorName = draftColor,
                                                size = 18.dp,
                                            )
                                            Spacer(Modifier.width(5.dp))
                                            Text(
                                                text = if (strings.isEnglish) key else label,
                                                color = if (selected) palette.selectedPillContent else palette.primaryText,
                                                fontSize = 11.sp,
                                                maxLines = 1,
                                            )
                                        }
                                    }
                                }
                            }
                        }

                        Text(
                            text = if (strings.isEnglish) "Palette (11)" else "配色方案 (11 色)",
                            color = palette.secondaryText,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            DesktopV166BotAvatarPalettes.take(6).forEach { (key, color) ->
                                val selected = draftColor.equals(key, ignoreCase = true)
                                Box(
                                    modifier = Modifier
                                        .size(30.dp)
                                        .clip(CircleShape)
                                        .background(color)
                                        .border(
                                            width = if (selected) 2.5.dp else 1.dp,
                                            color = if (selected) palette.primaryText else palette.borderColor,
                                            shape = CircleShape,
                                        )
                                        .clickable {
                                            draftColor = key
                                            customHexInput = ""
                                        },
                                )
                            }
                        }
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            DesktopV166BotAvatarPalettes.drop(6).forEach { (key, color) ->
                                val selected = draftColor.equals(key, ignoreCase = true)
                                Box(
                                    modifier = Modifier
                                        .size(30.dp)
                                        .clip(CircleShape)
                                        .background(color)
                                        .border(
                                            width = if (selected) 2.5.dp else 1.dp,
                                            color = if (selected) palette.primaryText else palette.borderColor,
                                            shape = CircleShape,
                                        )
                                        .clickable {
                                            draftColor = key
                                            customHexInput = ""
                                        },
                                )
                            }
                        }

                        // Custom #RRGGBB hex color input (Master Spec §8.8)
                        val normalizedCustomHex = customHexInput.trim().let { raw ->
                            if (raw.isNotEmpty() && !raw.startsWith("#")) "#$raw" else raw
                        }
                        val customHexValid = customHexInput.isBlank() || isValidHexColor(normalizedCustomHex)
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(28.dp)
                                    .clip(CircleShape)
                                    .background(resolveBotAvatarColor(draftColor, chat.id))
                                    .border(1.dp, palette.borderColor, CircleShape),
                            )
                            OutlinedTextField(
                                value = customHexInput,
                                onValueChange = { raw ->
                                    val capped = raw.trim().take(7)
                                    customHexInput = capped
                                    val candidate = capped.let { s ->
                                        if (s.isNotEmpty() && !s.startsWith("#")) "#$s" else s
                                    }
                                    if (isValidHexColor(candidate)) {
                                        draftColor = candidate.uppercase(Locale.ROOT)
                                    }
                                },
                                placeholder = {
                                    Text(
                                        text = if (strings.isEnglish) "Custom #RRGGBB (e.g. #1084FE)" else "自定义色值 #RRGGBB（如 #1084FE）",
                                        color = palette.hintText,
                                        fontSize = 12.sp,
                                    )
                                },
                                singleLine = true,
                                isError = !customHexValid,
                                colors = OutlinedTextFieldDefaults.colors(
                                    focusedTextColor = palette.primaryText,
                                    unfocusedTextColor = palette.primaryText,
                                    cursorColor = palette.searchCursorColor,
                                    focusedBorderColor = palette.primaryAccent,
                                    unfocusedBorderColor = palette.borderColor,
                                ),
                                modifier = Modifier
                                    .weight(1f)
                                    .testTag("muse-bot-custom-hex-input"),
                            )
                        }
                        if (!customHexValid) {
                            Text(
                                text = if (strings.isEnglish) "Use 6-digit hex format like #1084FE" else "请输入 6 位十六进制颜色，如 #1084FE",
                                color = palette.destructiveRed,
                                fontSize = 11.sp,
                            )
                        }

                        Button(
                            onClick = {
                                val resolvedColor = if (customHexInput.isNotBlank() && isValidHexColor(normalizedCustomHex)) {
                                    normalizedCustomHex.uppercase(Locale.ROOT)
                                } else {
                                    draftColor
                                }
                                onPatchBot(botIdKey, null, null, draftShape, resolvedColor, null, null)
                                editingAvatar = false
                            },
                            enabled = customHexValid,
                            colors = ButtonDefaults.buttonColors(
                                containerColor = palette.primaryAccent,
                                contentColor = Color.White,
                            ),
                            shape = RoundedCornerShape(10.dp),
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(if (strings.isEnglish) "Save Avatar Style" else "保存智能体头像样式")
                        }
                    }
                }
            }

            Spacer(Modifier.height(14.dp))

            // 4-Tab Segmented Pill Bar (Profile, Members/Agent, Shared Media, Settings)
            Surface(
                shape = RoundedCornerShape(12.dp),
                color = palette.inputBg,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(3.dp),
                    horizontalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    val tabs = listOf(
                        MuseInspectorTab.PROFILE to (TelegramGlyph.INFO_CIRCLE to strings.inspectorProfile),
                        MuseInspectorTab.MEMBERS_OR_AGENT to (
                            (if (isBotChat) TelegramGlyph.BOT_ROBOT else TelegramGlyph.CONTACTS) to
                                (if (isBotChat) strings.inspectorAgentComputers else strings.inspectorMembers)
                            ),
                        MuseInspectorTab.SHARED_MEDIA to (TelegramGlyph.MEDIA_PHOTO to strings.inspectorSharedMedia),
                        MuseInspectorTab.SETTINGS to (TelegramGlyph.SETTINGS_GEAR to strings.inspectorSettings),
                    )
                    tabs.forEach { (tab, iconLabel) ->
                        val selected = activeTab == tab
                        Surface(
                            shape = RoundedCornerShape(9.dp),
                            color = if (selected) palette.modalBg else Color.Transparent,
                            shadowElevation = if (selected) 1.dp else 0.dp,
                            modifier = Modifier
                                .weight(1f)
                                .clip(RoundedCornerShape(9.dp))
                                .clickable { activeTab = tab },
                        ) {
                            Column(
                                modifier = Modifier.padding(vertical = 7.dp),
                                horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.spacedBy(2.dp),
                            ) {
                                TelegramIcon(
                                    glyph = iconLabel.first,
                                    tint = if (selected) palette.primaryAccent else palette.secondaryText,
                                    size = 16.dp,
                                )
                                Text(
                                    text = iconLabel.second,
                                    color = if (selected) palette.primaryText else palette.secondaryText,
                                    fontSize = 10.sp,
                                    fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                    }
                }
            }

            Spacer(Modifier.height(14.dp))

            when (activeTab) {
                MuseInspectorTab.PROFILE -> {
                    MuseCollapsibleHeader(
                        title = if (strings.isEnglish) "About" else "简介与信息",
                        expanded = aboutExpanded,
                        onToggle = { aboutExpanded = !aboutExpanded },
                    )
                    if (aboutExpanded) {
                        val bio = chat.peerBio?.takeIf { it.isNotBlank() }
                            ?: fabushiBot?.description?.takeIf { it.isNotBlank() }
                            ?: chat.subtitle.takeIf { it.isNotBlank() }
                            ?: if (strings.isEnglish) "No description set." else "暂无简介说明。"
                        Surface(
                            shape = RoundedCornerShape(12.dp),
                            color = palette.cardBg,
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Column(
                                modifier = Modifier.padding(12.dp),
                                verticalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                Text(bio, color = palette.primaryText, fontSize = 14.sp, lineHeight = 20.sp)
                                if (handle != null) {
                                    MuseIconRow(
                                        glyph = TelegramGlyph.COPY,
                                        badgeColor = palette.primaryAccent,
                                        title = if (strings.isEnglish) "Copy Handle" else "复制用户名",
                                        subtitle = handle,
                                        onClick = {
                                            copyToClipboard(context, handle)
                                            onToast(if (strings.isEnglish) "Copied $handle" else "已复制 $handle")
                                        },
                                    )
                                }
                                if (chat.isSecretChat) {
                                    MuseIconRow(
                                        glyph = TelegramGlyph.SECRET_SHIELD,
                                        badgeColor = palette.secretGreen,
                                        title = if (strings.isEnglish) "Encryption Key" else "端到端加密指纹",
                                        subtitle = if (chat.secretVerified) "已核验" else "点击核验",
                                        onClick = {
                                            onDismiss()
                                            onOpenSecretVerify()
                                        },
                                    )
                                }
                                if (chat.kind == ConversationKind.CHANNEL) {
                                    MuseIconRow(
                                        glyph = TelegramGlyph.POLL_BARS,
                                        badgeColor = Color(0xFF5856D6),
                                        title = if (strings.isEnglish) "Channel Statistics" else "频道统计",
                                        subtitle = "${chat.subscriberCount} 订阅者",
                                        onClick = {
                                            onDismiss()
                                            onOpenChannelStats()
                                        },
                                    )
                                }
                            }
                        }
                    }

                    if (isBotChat) {
                        Spacer(Modifier.height(12.dp))
                        MuseCollapsibleHeader(
                            title = if (strings.isEnglish) "Quick Commands" else "快捷指令",
                            expanded = quickCmdsExpanded,
                            onToggle = { quickCmdsExpanded = !quickCmdsExpanded },
                        )
                        if (quickCmdsExpanded) {
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = palette.cardBg,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Column(modifier = Modifier.padding(8.dp)) {
                                    listOf("/help" to "查看能力与帮助", "/status" to "查看运行状态", "/reset" to "重置当前会话").forEach { (cmd, desc) ->
                                        MuseIconRow(
                                            glyph = TelegramGlyph.BOT_ROBOT,
                                            badgeColor = Color(0xFF34C759),
                                            title = cmd,
                                            subtitle = desc,
                                            onClick = {
                                                onDismiss()
                                                onSendQuickCommand(cmd)
                                            },
                                        )
                                    }
                                }
                            }
                        }
                    }
                }

                MuseInspectorTab.MEMBERS_OR_AGENT -> {
                    if (isBotChat) {
                        MuseCollapsibleHeader(
                            title = if (strings.isEnglish) "Agent Configuration" else "智能体配置",
                            expanded = agentConfigExpanded,
                            onToggle = { agentConfigExpanded = !agentConfigExpanded },
                        )
                        if (agentConfigExpanded) {
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = palette.cardBg,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Column(
                                    modifier = Modifier.padding(12.dp),
                                    verticalArrangement = Arrangement.spacedBy(8.dp),
                                ) {
                                    MuseIconRow(
                                        glyph = TelegramGlyph.HERMES_SPARK,
                                        badgeColor = Color(0xFFAF52DE),
                                        title = if (strings.isEnglish) "Model & Approval" else "模型与审批策略",
                                        subtitle = "${fabushiBot?.preferredModel?.ifBlank { "默认" } ?: "默认"} · ${fabushiBot?.approvalMode?.ifBlank { "ask" } ?: "ask"}",
                                        onClick = {
                                            if (fabushiBot != null && botIdKey.isNotBlank()) {
                                                editingBotConfig = !editingBotConfig
                                            }
                                        },
                                    )
                                    if (editingBotConfig && fabushiBot != null && botIdKey.isNotBlank()) {
                                        OutlinedTextField(
                                            value = draftModel,
                                            onValueChange = { draftModel = it },
                                            label = { Text("Preferred Model") },
                                            singleLine = true,
                                            colors = OutlinedTextFieldDefaults.colors(
                                                focusedTextColor = palette.primaryText,
                                                unfocusedTextColor = palette.primaryText,
                                                cursorColor = palette.searchCursorColor,
                                            ),
                                            modifier = Modifier.fillMaxWidth(),
                                        )
                                        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                            listOf("ask", "auto_edit", "full_access").forEach { mode ->
                                                val sel = draftApproval == mode
                                                Surface(
                                                    shape = RoundedCornerShape(8.dp),
                                                    color = if (sel) palette.selectedPillBackground else palette.inputBg,
                                                    modifier = Modifier
                                                        .clip(RoundedCornerShape(8.dp))
                                                        .clickable { draftApproval = mode },
                                                ) {
                                                    Text(
                                                        text = mode,
                                                        color = if (sel) palette.selectedPillContent else palette.primaryText,
                                                        fontSize = 11.sp,
                                                        modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
                                                    )
                                                }
                                            }
                                        }
                                        Button(
                                            onClick = {
                                                onPatchBot(
                                                    botIdKey,
                                                    null,
                                                    null,
                                                    null,
                                                    null,
                                                    draftModel.trim().ifBlank { null },
                                                    draftApproval,
                                                )
                                                editingBotConfig = false
                                            },
                                            colors = ButtonDefaults.buttonColors(
                                                containerColor = palette.primaryAccent,
                                                contentColor = Color.White,
                                            ),
                                            modifier = Modifier.fillMaxWidth(),
                                        ) {
                                            Text(strings.save)
                                        }
                                    }
                                }
                            }
                        }

                        Spacer(Modifier.height(12.dp))
                        MuseCollapsibleHeader(
                            title = if (strings.isEnglish) "Bound Computer" else "绑定远程电脑",
                            expanded = boundComputerExpanded,
                            onToggle = { boundComputerExpanded = !boundComputerExpanded },
                        )
                        if (boundComputerExpanded) {
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = palette.cardBg,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Column(
                                    modifier = Modifier.padding(10.dp),
                                    verticalArrangement = Arrangement.spacedBy(6.dp),
                                ) {
                                    if (computers.isEmpty()) {
                                        Text(
                                            text = if (strings.isEnglish) "No remote computers available yet." else "暂无可绑定的远程电脑节点，请先在「远程」标签页配置。",
                                            color = palette.secondaryText,
                                            fontSize = 13.sp,
                                            modifier = Modifier.padding(6.dp),
                                        )
                                    } else {
                                        computers.forEach { node ->
                                            val isBound = boundNodeId == node.id
                                            MuseIconRow(
                                                glyph = TelegramGlyph.CLOUD_COMPUTER,
                                                badgeColor = if (node.online) palette.secretGreen else palette.secondaryText,
                                                title = node.name,
                                                subtitle = buildString {
                                                    append(if (node.online) "在线" else "离线")
                                                    if (node.osInfo.isNotBlank()) append(" · ${node.osInfo}")
                                                },
                                                trailingText = if (isBound) "已绑定" else "绑定",
                                                onClick = {
                                                    if (botIdKey.isNotBlank()) {
                                                        onBindBotComputer(botIdKey, if (isBound) null else node.id, isBound)
                                                    }
                                                },
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        MuseCollapsibleHeader(
                            title = if (strings.isEnglish) "Participants" else "会话成员",
                            expanded = participantsExpanded,
                            onToggle = { participantsExpanded = !participantsExpanded },
                        )
                        if (participantsExpanded) {
                            Surface(
                                shape = RoundedCornerShape(12.dp),
                                color = palette.cardBg,
                                modifier = Modifier.fillMaxWidth(),
                            ) {
                                Column(modifier = Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                    val count = when (chat.kind) {
                                        ConversationKind.CHANNEL -> chat.subscriberCount
                                        ConversationKind.GROUP -> chat.memberCount
                                        else -> 2
                                    }
                                    Text(
                                        text = if (strings.isEnglish) "$count participant(s)" else "共 $count 位成员",
                                        color = palette.primaryText,
                                        fontWeight = FontWeight.Medium,
                                        fontSize = 14.sp,
                                    )
                                    Text(
                                        text = chat.title,
                                        color = palette.secondaryText,
                                        fontSize = 13.sp,
                                    )
                                }
                            }
                        }
                    }
                }

                MuseInspectorTab.SHARED_MEDIA -> {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        MuseMediaFilter.entries.forEach { filter ->
                            val selected = mediaFilter == filter
                            Surface(
                                shape = RoundedCornerShape(10.dp),
                                color = if (selected) palette.selectedPillBackground else palette.inputBg,
                                modifier = Modifier
                                    .weight(1f)
                                    .clip(RoundedCornerShape(10.dp))
                                    .clickable { mediaFilter = filter },
                            ) {
                                Box(
                                    modifier = Modifier.padding(vertical = 6.dp),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text(
                                        text = if (strings.isEnglish) filter.labelEn else filter.labelZh,
                                        color = if (selected) palette.selectedPillContent else palette.primaryText,
                                        fontSize = 11.sp,
                                        fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                                        maxLines = 1,
                                    )
                                }
                            }
                        }
                    }
                    Spacer(Modifier.height(10.dp))
                    val uriHandler = LocalUriHandler.current
                    val filteredMedia = remember(messages, mediaFilter) {
                        when (mediaFilter) {
                            MuseMediaFilter.MEDIA -> messages.filter { it.contentType in setOf("photo", "video", "videoNote") }
                            MuseMediaFilter.FILES -> messages.filter {
                                it.contentType in setOf("file", "document") ||
                                    (it.mediaFileName != null && it.contentType !in setOf("photo", "video", "voice"))
                            }
                            MuseMediaFilter.VOICE -> messages.filter { it.contentType == "voice" }
                            MuseMediaFilter.LINKS -> messages.filter {
                                extractFirstSafeHttpUrl(it.text, it.entities) != null
                            }
                            MuseMediaFilter.MUSIC -> messages.filter { it.mediaMimeType?.startsWith("audio/") == true && it.contentType != "voice" }
                        }
                    }
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = palette.cardBg,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        if (filteredMedia.isEmpty()) {
                            Box(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(24.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    text = if (strings.isEnglish) "No shared items in this category." else "该分类暂无共享内容",
                                    color = palette.secondaryText,
                                    fontSize = 13.sp,
                                )
                            }
                        } else {
                            Column(modifier = Modifier.padding(8.dp)) {
                                filteredMedia.take(20).forEachIndexed { idx, item ->
                                    val glyph = when (mediaFilter) {
                                        MuseMediaFilter.MEDIA -> TelegramGlyph.MEDIA_PHOTO
                                        MuseMediaFilter.FILES -> TelegramGlyph.DOCUMENT_FILE
                                        MuseMediaFilter.VOICE -> TelegramGlyph.MIC
                                        MuseMediaFilter.LINKS -> TelegramGlyph.LINK_CHAIN
                                        MuseMediaFilter.MUSIC -> TelegramGlyph.MUSIC_NOTE
                                    }
                                    MuseIconRow(
                                        glyph = glyph,
                                        badgeColor = palette.primaryAccent,
                                        title = item.mediaFileName ?: item.text.take(44).ifBlank { item.contentType },
                                        subtitle = "${item.senderName} · ${item.time}",
                                        onClick = {
                                            if (mediaFilter == MuseMediaFilter.LINKS) {
                                                val link = extractFirstSafeHttpUrl(item.text, item.entities)
                                                if (link != null) {
                                                    runCatching { uriHandler.openUri(link) }
                                                        .onFailure {
                                                            copyToClipboard(context, link)
                                                            onToast(if (strings.isEnglish) "Copied link" else "已复制链接")
                                                        }
                                                }
                                            } else if (item.mediaBlobId != null || item.contentType in setOf("photo", "video", "file", "document")) {
                                                onDismiss()
                                                onOpenMediaViewer(item)
                                            }
                                        },
                                    )
                                    if (idx < filteredMedia.lastIndex && idx < 19) {
                                        TelegramHairlineDivider(Modifier.padding(start = 46.dp))
                                    }
                                }
                            }
                        }
                    }
                }

                MuseInspectorTab.SETTINGS -> {
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = palette.cardBg,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column(modifier = Modifier.padding(8.dp)) {
                            MuseIconRow(
                                glyph = TelegramGlyph.PIN,
                                badgeColor = palette.primaryAccent,
                                title = if (chat.isPinned) strings.unpinAction else strings.pinAction,
                                subtitle = if (chat.isPinned) "已置顶到会话列表顶部" else "置顶此对话",
                                onClick = onTogglePin,
                            )
                            TelegramHairlineDivider(Modifier.padding(start = 46.dp))
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(horizontal = 8.dp, vertical = 6.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(30.dp)
                                        .clip(CircleShape)
                                        .background(Color(0xFFFF9500)),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    TelegramIcon(
                                        glyph = if (chat.isMuted) TelegramGlyph.BELL_SLASH else TelegramGlyph.BELL,
                                        tint = Color.White,
                                        size = 15.dp,
                                    )
                                }
                                Spacer(Modifier.width(10.dp))
                                Column(Modifier.weight(1f)) {
                                    Text(
                                        if (strings.isEnglish) "Mute Notifications" else "静音通知",
                                        color = palette.primaryText,
                                        fontSize = 14.sp,
                                        fontWeight = FontWeight.Medium,
                                    )
                                }
                                Switch(checked = chat.isMuted, onCheckedChange = { onToggleMute() })
                            }
                            TelegramHairlineDivider(Modifier.padding(start = 46.dp))
                            val nextTtl = when (chat.ttlSeconds) {
                                0 -> 30
                                30 -> 3600
                                3600 -> 86400
                                else -> 0
                            }
                            MuseIconRow(
                                glyph = TelegramGlyph.LOCK_CLOSED,
                                badgeColor = palette.secretGreen,
                                title = if (strings.isEnglish) "Auto-Delete Timer" else "阅后即焚定时",
                                subtitle = if (chat.ttlSeconds > 0) "${chat.ttlSeconds}s" else "关闭",
                                onClick = { onSetTtl(nextTtl) },
                            )
                            TelegramHairlineDivider(Modifier.padding(start = 46.dp))
                            MuseIconRow(
                                glyph = TelegramGlyph.TRASH,
                                badgeColor = palette.destructiveRed,
                                title = if (strings.isEnglish) "Clear History" else "清空聊天记录",
                                titleColor = palette.destructiveRed,
                                onClick = {
                                    onClearHistory()
                                    onDismiss()
                                },
                            )
                            if (chat.kind == ConversationKind.GROUP || chat.kind == ConversationKind.CHANNEL) {
                                TelegramHairlineDivider(Modifier.padding(start = 46.dp))
                                MuseIconRow(
                                    glyph = TelegramGlyph.SIGN_OUT,
                                    badgeColor = palette.destructiveRed,
                                    title = if (strings.isEnglish) "Leave Conversation" else "退出群组 / 频道",
                                    titleColor = palette.destructiveRed,
                                    onClick = {
                                        onLeaveChat()
                                        onDismiss()
                                    },
                                )
                            }
                        }
                    }
                }
            }
            Spacer(Modifier.height(14.dp))
        }
    }
}

@Composable
private fun MuseCollapsibleHeader(
    title: String,
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .clickable(onClick = onToggle)
            .padding(horizontal = 4.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = title,
            color = palette.secondaryText,
            fontSize = 12.sp,
            fontWeight = FontWeight.SemiBold,
        )
        TelegramIcon(
            glyph = if (expanded) TelegramGlyph.CHEVRON_DOWN else TelegramGlyph.CHEVRON_RIGHT,
            tint = palette.secondaryText,
            size = 12.dp,
        )
    }
}

@Composable
private fun MuseIconRow(
    glyph: TelegramGlyph,
    badgeColor: Color,
    title: String,
    subtitle: String? = null,
    trailingText: String? = null,
    titleColor: Color? = null,
    onClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(30.dp)
                .clip(CircleShape)
                .background(badgeColor),
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(glyph = glyph, tint = Color.White, size = 15.dp)
        }
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            Text(
                text = title,
                color = titleColor ?: palette.primaryText,
                fontSize = 14.sp,
                fontWeight = FontWeight.Medium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (!subtitle.isNullOrBlank()) {
                Text(
                    text = subtitle,
                    color = palette.secondaryText,
                    fontSize = 12.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (!trailingText.isNullOrBlank()) {
            Text(
                text = trailingText,
                color = palette.primaryAccent,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
            )
            Spacer(Modifier.width(4.dp))
        }
        TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 14.dp)
    }
}

private fun copyToClipboard(context: Context, text: String) {
    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
    cm.setPrimaryClip(ClipData.newPlainText("fabushi", text))
}

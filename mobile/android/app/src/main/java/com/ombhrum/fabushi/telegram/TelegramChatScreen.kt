package com.ombhrum.fabushi.telegram

import android.Manifest
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.BitmapFactory
import android.net.Uri
import android.util.Base64
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import com.ombhrum.fabushi.ConversationKind
import kotlinx.coroutines.launch
import java.io.File

/**
 * Apple iOS Messages Chat Stage (`F14`, Master Spec §8.8):
 * - Root `Box` hosts ALL overlays (hoisted Attachment Sheet, Muse 4-Tab Inspector, Message Context Menu,
 *   Forward Picker, Channel Stats, Discussion Sheet, Poll & Contact dialogs) — NEVER inside `Column`!
 * - Centered `76dp` frosted glass contact header (`Title ›` opens `MuseInspectorSheet` for ALL chats;
 *   zero fake call buttons; Export JSON launches Android `Intent.ACTION_SEND` share sheet)
 * - Fixed `46dp` Pinned Message Banner jumping accurately to `timelineItems` index
 * - `buildTimelineItems` calendar-day dividers + `18dp` gradient pill bubbles with ALL 12 outgoing contrast
 *   defects fixed, deduplicated code cards via `parseMessageCodeSegments`, real Base64 thumbnail decoding,
 *   floating `26dp` Tapbacks, Mika inline Approve/Decline buttons, failed-message retry, and smart auto-scroll.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TelegramChatScreen(
    state: TelegramUiState,
    chat: TelegramChatItem,
    messages: List<TelegramMessageItem>,
    topics: List<TelegramForumTopic>,
    totalUnreadCount: Int,
    onBack: () -> Unit,
    onSelectTopic: (String?) -> Unit,
    onSetInChatSearch: (Boolean, String) -> Unit,
    onUpdateDraft: (String) -> Unit,
    onSetReplyTarget: (TelegramMessageItem?) -> Unit,
    onSetEditTarget: (TelegramMessageItem?) -> Unit,
    onSendMessage: (text: String, silent: Boolean, scheduledAtMs: Long?, overrideTtl: Int?) -> Unit,
    onStartBotChat: () -> Unit = {},
    onOpenMiniApp: (String) -> Unit = {},
    onRetryFailedMessage: (String) -> Unit,
    onLoadOlderMessages: () -> Unit,
    onToggleReaction: (String, String) -> Unit,
    onTogglePinMessage: (String) -> Unit,
    onCyclePinnedBanner: () -> String?,
    onDeleteMessage: (String, Boolean) -> Unit,
    onForwardMessage: (TelegramMessageItem, String) -> Unit,
    onTranslateMessage: (String) -> Unit,
    onSendSticker: (String, String) -> Unit,
    onSendGif: (GifItemModel) -> Unit,
    onSendPoll: (String, List<String>, Boolean, Boolean, Boolean, Int, String?) -> Unit,
    onVotePoll: (String, String) -> Unit,
    onSendLocation: (Double, Double, String) -> Unit,
    onSendContact: (String, String) -> Unit,
    onSendMedia: (String, String, ByteArray) -> Unit,
    onOpenMediaViewer: (TelegramMessageItem) -> Unit,
    onClickInlineButton: (messageId: String, button: InlineKeyboardButtonModel) -> Unit,
    onResolveMikaConfirmation: (approved: Boolean, threadId: String?, askId: String?) -> Unit,
    onSetChatTtl: (Int) -> Unit,
    onClearChatHistory: () -> Unit,
    onLeaveChat: () -> Unit,
    onToggleMuteChat: () -> Unit,
    onTogglePinChat: () -> Unit,
    onExportChatHistory: (String) -> File?,
    onOpenSecretVerify: () -> Unit,
    onOpenChannelStats: () -> Unit,
    onCloseChannelStats: () -> Unit,
    onOpenDiscussion: (TelegramMessageItem?) -> Unit,
    onPostDiscussionComment: (TelegramMessageItem, String) -> Unit,
    onToggleRecordMode: () -> Unit,
    onStartRecording: () -> Unit,
    onDragRecording: (Float, Float) -> Unit,
    onCancelRecording: () -> Unit,
    onFinishRecording: () -> Unit,
    onPlayOrToggleAudio: (TelegramMessageItem, Float?) -> Unit,
    onCycleAudioSpeed: () -> Unit,
    onStopAudio: () -> Unit,
    onSetAttachmentSheetOpen: (Boolean) -> Unit,
    onPatchFabushiBot: (
        botId: String,
        displayName: String?,
        description: String?,
        avatarShape: String?,
        avatarColor: String?,
        preferredModel: String?,
        approvalMode: String?,
    ) -> Unit,
    onBindBotComputer: (botId: String, nodeId: String?, clear: Boolean) -> Unit,
    onRefreshBotComputer: (botId: String) -> Unit = {},
    onToast: (String) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val listState = rememberLazyListState()

    var showMuseInspector by remember { mutableStateOf(false) }
    var showOverflowMenu by remember { mutableStateOf(false) }
    var contextMenuMessage by remember { mutableStateOf<TelegramMessageItem?>(null) }
    var forwardMessageTarget by remember { mutableStateOf<TelegramMessageItem?>(null) }
    var showPollCreator by remember { mutableStateOf(false) }
    var showContactDialog by remember { mutableStateOf(false) }

    BackHandler {
        when {
            showMuseInspector -> showMuseInspector = false
            state.composerAttachmentSheetOpen -> onSetAttachmentSheetOpen(false)
            contextMenuMessage != null -> contextMenuMessage = null
            forwardMessageTarget != null -> forwardMessageTarget = null
            state.inChatSearchActive -> onSetInChatSearch(false, "")
            else -> onBack()
        }
    }

    // Media picker launcher (real Android SAF)
    val mediaPickerLauncher = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri != null) {
            runCatching {
                val mime = context.contentResolver.getType(uri) ?: "application/octet-stream"
                val name = uri.lastPathSegment?.substringAfterLast('/')?.ifBlank { "attachment" } ?: "attachment"
                val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: ByteArray(0)
                if (bytes.isNotEmpty()) {
                    onSendMedia(name, mime, bytes)
                } else {
                    onToast("所选文件为空，无法发送")
                }
            }.onFailure {
                onToast("读取附件失败：${it.message ?: "未知错误"}")
            }
        }
    }

    // Real GPS permission launcher (zero fake Hangzhou fallback!)
    val locationPermLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) { grants ->
        if (grants.values.any { it }) {
            val coords = fetchRealDeviceLocationOrNull(context)
            if (coords != null) {
                onSendLocation(coords.first, coords.second, if (strings.isEnglish) "Current Location" else "当前位置")
            } else {
                onToast(if (strings.isEnglish) "Unable to acquire GPS fix." else "暂未获取到有效定位，请确认已开启定位服务")
            }
        } else {
            onToast(if (strings.isEnglish) "Location permission denied." else "未授予位置权限，无法发送定位")
        }
    }

    // Voice permission launcher for attachment sheet Quick Voice
    val voicePermLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) {
            onStartRecording()
        } else {
            onToast(if (strings.isEnglish) "Microphone permission denied." else "未授予麦克风权限，无法录制语音")
        }
    }

    val filteredMessages = remember(messages, state.activeTopicId, state.inChatSearchActive, state.inChatSearchQuery) {
        messages.filter { msg ->
            val topicOk = state.activeTopicId == null || msg.topicId == null || msg.topicId == state.activeTopicId
            val searchOk = !state.inChatSearchActive || state.inChatSearchQuery.isBlank() ||
                msg.text.contains(state.inChatSearchQuery, ignoreCase = true) ||
                msg.senderName.contains(state.inChatSearchQuery, ignoreCase = true)
            topicOk && searchOk
        }
    }

    val timelineItems = remember(filteredMessages, state.uiLanguage) {
        buildTimelineItems(filteredMessages, uiLanguage = state.uiLanguage)
    }
    val showLoadOlderHeader = chat.hasMoreHistory && state.isAuthenticated && filteredMessages.isNotEmpty()
    val headerOffset = if (showLoadOlderHeader) 1 else 0

    // Smart auto-scroll: initial open or when newest message appends while near bottom
    val newestMessageId = filteredMessages.lastOrNull()?.id
    val newestOutgoing = filteredMessages.lastOrNull()?.outgoing == true
    var prevChatId by remember { mutableStateOf<String?>(null) }
    var prevNewestId by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(chat.id, newestMessageId) {
        val totalItems = timelineItems.size + headerOffset
        if (totalItems == 0) return@LaunchedEffect
        val switchedChat = prevChatId != chat.id
        val appendedNew = prevNewestId != null && newestMessageId != prevNewestId
        val lastVisible = listState.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0
        val nearBottom = (totalItems - 1 - lastVisible) <= 3
        if (switchedChat) {
            listState.scrollToItem((totalItems - 1).coerceAtLeast(0))
        } else if (appendedNew && (nearBottom || newestOutgoing) && !state.inChatSearchActive) {
            listState.animateScrollToItem((totalItems - 1).coerceAtLeast(0))
        }
        prevChatId = chat.id
        prevNewestId = newestMessageId
    }

    val matchingFabushiBot = remember(state.fabushiBots, chat) {
        state.fabushiBots.firstOrNull { bot ->
            (bot.companionChatId != null && bot.companionChatId.toString() == chat.id) ||
                (chat.peerUserId != null && bot.botUserId == chat.peerUserId) ||
                matchesBotUsername(chat.username, bot.username)
        }
    }
    val botIdKey = matchingFabushiBot?.let {
        if (it.botUserId > 0L) it.botUserId.toString() else it.agentId.ifBlank { it.username }
    }.orEmpty()

    LaunchedEffect(showMuseInspector, botIdKey) {
        if (showMuseInspector && botIdKey.isNotBlank()) {
            onRefreshBotComputer(botIdKey)
        }
    }

    val pinnedMessages = remember(chat.pinnedMessageIds, messages) {
        chat.pinnedMessageIds.mapNotNull { pid -> messages.firstOrNull { it.id == pid } }
    }
    val pinnedBannerIdx = (state.pinnedBannerIndexByChat[chat.id] ?: 0).coerceIn(0, (pinnedMessages.size - 1).coerceAtLeast(0))
    val activePin = pinnedMessages.getOrNull(pinnedBannerIdx)

    val liveBotStatus = formatBotActivitySubtitle(
        executionState = chat.botExecutionState.ifBlank { matchingFabushiBot?.executionState.orEmpty() },
        detail = chat.botActivityDetail,
        uiLanguage = state.uiLanguage,
    )
    val headerSubtitle = remember(chat, liveBotStatus, strings.isEnglish) {
        when {
            !chat.typingLabel.isNullOrBlank() -> chat.typingLabel
            liveBotStatus != null -> liveBotStatus
            chat.kind == ConversationKind.CHANNEL && chat.subscriberCount > 0 ->
                if (strings.isEnglish) "${chat.subscriberCount} subscribers" else "${chat.subscriberCount} 位订阅者"
            chat.kind == ConversationKind.GROUP && chat.memberCount > 0 ->
                if (strings.isEnglish) "${chat.memberCount} members" else "${chat.memberCount} 位成员"
            chat.ttlSeconds > 0 -> if (strings.isEnglish) "Auto-delete ${chat.ttlSeconds}s" else "阅后即焚 ${chat.ttlSeconds}s"
            else -> null // Hidden when idle on DM per Desktop v1.6.6
        }
    }

    // Root Box ensures ALL overlays sit cleanly above Column without ever crushing weight(1f)
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-chat-screen"),
    ) {
        Column(modifier = Modifier.fillMaxSize()) {
            // 1. Desktop v1.6.6 Centered 76dp Frosted Glass Contact Header
            Surface(
                color = palette.topBarBackground.copy(alpha = 0.94f),
                modifier = Modifier.fillMaxWidth(),
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .statusBarsPadding()
                        .height(76.dp)
                        .padding(horizontal = 12.dp),
                ) {
                    // Left glass Back pill + total unread badge
                    Surface(
                        shape = RoundedCornerShape(18.dp),
                        color = palette.inputBg.copy(alpha = 0.90f),
                        modifier = Modifier
                            .align(Alignment.CenterStart)
                            .clip(RoundedCornerShape(18.dp))
                            .clickable(onClick = onBack)
                            .testTag("chat-back-button"),
                    ) {
                        Row(
                            modifier = Modifier.padding(horizontal = 10.dp, vertical = 7.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            TelegramIcon(TelegramGlyph.CHEVRON_LEFT, tint = palette.topBarActionColor, size = 16.dp)
                            if (totalUnreadCount > 0) {
                                Spacer(Modifier.width(4.dp))
                                Box(
                                    modifier = Modifier
                                        .clip(CircleShape)
                                        .background(palette.unreadBadgeActive)
                                        .padding(horizontal = 6.dp, vertical = 1.dp),
                                ) {
                                    Text(
                                        text = if (totalUnreadCount > 99) "99+" else "$totalUnreadCount",
                                        color = palette.unreadBadgeContent,
                                        fontSize = 10.sp,
                                        fontWeight = FontWeight.Bold,
                                    )
                                }
                            }
                        }
                    }

                    // Center 40dp Avatar + overlapping -4dp frosted `Title ›` pill -> Muse Inspector
                    // Symmetric 104dp horizontal clearance guarantees zero overlap with left/right pills on 360dp screens
                    Column(
                        modifier = Modifier
                            .align(Alignment.Center)
                            .padding(horizontal = 104.dp)
                            .widthIn(max = 200.dp)
                            .clip(RoundedCornerShape(14.dp))
                            .clickable { showMuseInspector = true }
                            .testTag("chat-header-title-pill"),
                        horizontalAlignment = Alignment.CenterHorizontally,
                    ) {
                        TelegramAvatar(
                            idSeed = chat.id,
                            title = chat.title,
                            size = 40.dp,
                            kind = chat.kind,
                            online = chat.online,
                            isSecret = chat.isSecretChat,
                            isCompanionGhost = chat.isCompanionGhost && !chat.isBot,
                            avatarShape = chat.avatarShape ?: matchingFabushiBot?.avatarShape,
                            avatarColor = chat.avatarColor ?: matchingFabushiBot?.avatarColor,
                            isBot = chat.isBot || matchingFabushiBot != null,
                            botActivityActive = liveBotStatus != null,
                        )
                        Surface(
                            shape = RoundedCornerShape(12.dp),
                            color = palette.cardBg.copy(alpha = 0.94f),
                            shadowElevation = if (palette.isDark) 0.dp else 1.dp,
                            modifier = Modifier
                                .offset(y = (-4).dp)
                                .border(0.5.dp, palette.borderColor, RoundedCornerShape(12.dp)),
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 9.dp, vertical = 2.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    text = chat.title,
                                    color = palette.topBarContent,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.weight(1f, fill = false),
                                )
                                Spacer(Modifier.width(2.dp))
                                TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.secondaryText, size = 10.dp)
                            }
                        }
                        if (!headerSubtitle.isNullOrBlank()) {
                            Text(
                                text = headerSubtitle,
                                color = if (liveBotStatus != null || !chat.typingLabel.isNullOrBlank()) palette.primaryAccent else palette.secondaryText,
                                fontSize = 10.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                                modifier = Modifier.offset(y = (-3).dp),
                            )
                        }
                    }

                    // Right compact glass action pill: Search | Inspector | More (ZERO fake call buttons!)
                    Box(modifier = Modifier.align(Alignment.CenterEnd)) {
                        Surface(
                            shape = RoundedCornerShape(18.dp),
                            color = palette.inputBg.copy(alpha = 0.90f),
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 6.dp, vertical = 5.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(2.dp),
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(28.dp)
                                        .clip(CircleShape)
                                        .clickable { onSetInChatSearch(!state.inChatSearchActive, "") },
                                    contentAlignment = Alignment.Center,
                                ) {
                                    TelegramIcon(TelegramGlyph.SEARCH, tint = palette.topBarActionColor, size = 15.dp)
                                }
                                Box(
                                    modifier = Modifier
                                        .size(28.dp)
                                        .clip(CircleShape)
                                        .clickable { showMuseInspector = true }
                                        .testTag("chat-header-inspector-button"),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    TelegramIcon(TelegramGlyph.INFO_CIRCLE, tint = palette.topBarActionColor, size = 16.dp)
                                }
                                Box(
                                    modifier = Modifier
                                        .size(28.dp)
                                        .clip(CircleShape)
                                        .clickable { showOverflowMenu = true },
                                    contentAlignment = Alignment.Center,
                                ) {
                                    TelegramIcon(TelegramGlyph.MORE_HORIZ, tint = palette.topBarActionColor, size = 16.dp)
                                }
                            }
                        }

                        DropdownMenu(
                            expanded = showOverflowMenu,
                            onDismissRequest = { showOverflowMenu = false },
                        ) {
                            DropdownMenuItem(
                                text = { Text(if (chat.isPinned) strings.unpinAction else strings.pinAction) },
                                onClick = {
                                    showOverflowMenu = false
                                    onTogglePinChat()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text(if (chat.isMuted) strings.unmuteAction else strings.muteAction) },
                                onClick = {
                                    showOverflowMenu = false
                                    onToggleMuteChat()
                                },
                            )
                            DropdownMenuItem(
                                text = { Text(if (strings.isEnglish) "Export JSON & Share" else "导出聊天记录 (JSON)") },
                                onClick = {
                                    showOverflowMenu = false
                                    val exported = onExportChatHistory("json")
                                    if (exported != null) {
                                        shareChatExportFile(context, exported, "application/json")
                                    } else {
                                        onToast(if (strings.isEnglish) "Failed to export chat." else "导出聊天记录失败")
                                    }
                                },
                            )
                            if (chat.isSecretChat) {
                                DropdownMenuItem(
                                    text = { Text("验证加密指纹") },
                                    onClick = {
                                        showOverflowMenu = false
                                        onOpenSecretVerify()
                                    },
                                )
                            }
                            if (chat.kind == ConversationKind.CHANNEL) {
                                DropdownMenuItem(
                                    text = { Text("频道统计") },
                                    onClick = {
                                        showOverflowMenu = false
                                        onOpenChannelStats()
                                    },
                                )
                            }
                            DropdownMenuItem(
                                text = { Text(if (strings.isEnglish) "Clear History" else "清空聊天记录", color = palette.destructiveRed) },
                                onClick = {
                                    showOverflowMenu = false
                                    onClearChatHistory()
                                },
                            )
                        }
                    }
                }
            }
            TelegramHairlineDivider()

            // Optional In-Chat Search Bar
            if (state.inChatSearchActive) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(palette.topBarBackground)
                        .padding(horizontal = 12.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Row(
                        modifier = Modifier
                            .weight(1f)
                            .height(34.dp)
                            .clip(RoundedCornerShape(10.dp))
                            .background(palette.inputBg)
                            .padding(horizontal = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        TelegramIcon(TelegramGlyph.SEARCH, tint = palette.secondaryText, size = 15.dp)
                        Spacer(Modifier.width(8.dp))
                        Box(Modifier.weight(1f)) {
                            if (state.inChatSearchQuery.isEmpty()) {
                                Text(strings.searchInChatPlaceholder, color = palette.secondaryText, fontSize = 14.sp)
                            }
                            BasicTextField(
                                value = state.inChatSearchQuery,
                                onValueChange = { onSetInChatSearch(true, it) },
                                singleLine = true,
                                textStyle = TextStyle(color = palette.primaryText, fontSize = 14.sp),
                                cursorBrush = SolidColor(palette.searchCursorColor),
                                modifier = Modifier.fillMaxWidth(),
                            )
                        }
                    }
                    TextButton(onClick = { onSetInChatSearch(false, "") }) {
                        Text(strings.cancel, color = palette.topBarActionColor, fontSize = 13.sp)
                    }
                }
                TelegramHairlineDivider()
            }

            // Optional Forum Topics Strip
            if (chat.isForum && topics.isNotEmpty()) {
                LazyRow(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(palette.topBarBackground)
                        .padding(horizontal = 12.dp, vertical = 6.dp),
                    horizontalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    item {
                        val sel = state.activeTopicId == null
                        Surface(
                            shape = RoundedCornerShape(12.dp),
                            color = if (sel) palette.selectedPillBackground else palette.inputBg,
                            modifier = Modifier.clip(RoundedCornerShape(12.dp)).clickable { onSelectTopic(null) },
                        ) {
                            Text(
                                text = if (strings.isEnglish) "All Topics" else "全部话题",
                                color = if (sel) palette.selectedPillContent else palette.primaryText,
                                fontSize = 12.sp,
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                            )
                        }
                    }
                    items(topics, key = { it.id }) { topic ->
                        val sel = state.activeTopicId == topic.id
                        Surface(
                            shape = RoundedCornerShape(12.dp),
                            color = if (sel) palette.selectedPillBackground else palette.inputBg,
                            modifier = Modifier.clip(RoundedCornerShape(12.dp)).clickable { onSelectTopic(topic.id) },
                        ) {
                            Text(
                                text = topic.title,
                                color = if (sel) palette.selectedPillContent else palette.primaryText,
                                fontSize = 12.sp,
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 5.dp),
                            )
                        }
                    }
                }
                TelegramHairlineDivider()
            }

            // 2. Fixed 46dp Pinned Message Banner (real server pins, media-only preview fallback, exact timeline jump!)
            if (activePin != null) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(46.dp)
                        .background(palette.cardBg)
                        .clickable {
                            val targetId = onCyclePinnedBanner() ?: activePin.id
                            val timelineIdx = timelineItems.indexOfFirst {
                                it is ChatTimelineItem.MessageEntry && it.message.id == targetId
                            }
                            if (timelineIdx >= 0) {
                                scope.launch { listState.animateScrollToItem(timelineIdx + headerOffset) }
                            }
                        }
                        .padding(horizontal = 14.dp)
                        .testTag("chat-pinned-banner"),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(
                        modifier = Modifier
                            .width(2.5.dp)
                            .height(26.dp)
                            .clip(RoundedCornerShape(2.dp))
                            .background(palette.primaryAccent),
                    )
                    Spacer(Modifier.width(10.dp))
                    TelegramIcon(TelegramGlyph.PIN, tint = palette.primaryAccent, size = 14.dp)
                    Spacer(Modifier.width(8.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = if (pinnedMessages.size > 1) {
                                "${strings.pinnedMessage} ${pinnedBannerIdx + 1}/${pinnedMessages.size}"
                            } else {
                                strings.pinnedMessage
                            },
                            color = palette.primaryAccent,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                        val pinPreview = activePin.text.lineSequence().firstOrNull()?.takeIf { it.isNotBlank() }
                            ?: activePin.mediaFileName?.takeIf { it.isNotBlank() }
                            ?: when (activePin.contentType) {
                                "photo" -> "[图片]"
                                "video", "videoNote" -> "[视频]"
                                "voice" -> "[语音消息]"
                                "poll" -> activePin.poll?.question ?: "[投票]"
                                "location" -> activePin.venueTitle ?: "[位置]"
                                "contact" -> activePin.contactName ?: "[联系人]"
                                else -> "[附件]"
                            }
                        Text(
                            text = pinPreview,
                            color = palette.primaryText,
                            fontSize = 12.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                    Box(
                        modifier = Modifier
                            .size(28.dp)
                            .clip(CircleShape)
                            .clickable { onTogglePinMessage(activePin.id) },
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.CLOSE, tint = palette.secondaryText, size = 14.dp)
                    }
                }
                TelegramHairlineDivider()
            }

            // Optional Floating Audio Playback Strip
            if (state.audioPlayback.active && state.audioPlayback.messageId.isNotBlank()) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(palette.cardBg)
                        .padding(horizontal = 14.dp, vertical = 7.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    TelegramIcon(
                        glyph = if (state.audioPlayback.isPlaying) TelegramGlyph.PAUSE else TelegramGlyph.PLAY,
                        tint = palette.primaryAccent,
                        size = 16.dp,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        text = state.audioPlayback.title,
                        color = palette.primaryText,
                        fontSize = 12.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(onClick = onCycleAudioSpeed) {
                        Text("${state.audioPlayback.speedMultiplier}x", color = palette.topBarActionColor, fontSize = 12.sp)
                    }
                    Box(
                        modifier = Modifier.size(26.dp).clip(CircleShape).clickable(onClick = onStopAudio),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.CLOSE, tint = palette.secondaryText, size = 14.dp)
                    }
                }
                TelegramHairlineDivider()
            }

            val isUnstartedBot = chat.isBot &&
                filteredMessages.isEmpty() &&
                !state.inChatSearchActive &&
                chat.id !in state.startedBotChatIds

            // 3. Monochrome Apple iOS Message Stage (weight(1f))
            TelegramChatWallpaperBackground(modifier = Modifier.weight(1f)) {
                if (timelineItems.isEmpty()) {
                    if (isUnstartedBot) {
                        val botDescription = matchingFabushiBot?.description?.takeIf { it.isNotBlank() }
                            ?: chat.peerBio?.takeIf { it.isNotBlank() }
                            ?: chat.subtitle.takeIf { it.isNotBlank() }
                            ?: if (strings.isEnglish) {
                                "Tap START below to initialize this bot and view its available commands."
                            } else {
                                "点击下方「开始」唤醒机器人并查看可用指令与能力。"
                            }
                        Box(
                            modifier = Modifier.fillMaxSize().padding(horizontal = 24.dp, vertical = 24.dp),
                            contentAlignment = Alignment.Center,
                        ) {
                            Surface(
                                shape = RoundedCornerShape(18.dp),
                                color = palette.cardBg,
                                border = BorderStroke(0.5.dp, palette.borderColor),
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .widthIn(max = 360.dp)
                                    .testTag("bot-what-can-i-do-card"),
                            ) {
                                Column(
                                    modifier = Modifier.padding(horizontal = 20.dp, vertical = 22.dp),
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    verticalArrangement = Arrangement.spacedBy(10.dp),
                                ) {
                                    TelegramAvatar(
                                        idSeed = chat.id,
                                        title = chat.title,
                                        size = 64.dp,
                                        isBot = true,
                                        avatarShape = matchingFabushiBot?.avatarShape ?: chat.avatarShape,
                                        avatarColor = matchingFabushiBot?.avatarColor ?: chat.avatarColor,
                                    )
                                    Text(
                                        text = strings.botWhatCanIDo,
                                        color = palette.primaryText,
                                        fontSize = 16.sp,
                                        fontWeight = FontWeight.SemiBold,
                                    )
                                    Text(
                                        text = chat.title,
                                        color = palette.primaryText,
                                        fontSize = 14.sp,
                                        fontWeight = FontWeight.Medium,
                                    )
                                    chat.username?.takeIf { it.isNotBlank() }?.let { uname ->
                                        Text(
                                            text = if (uname.startsWith("@")) uname else "@$uname",
                                            color = palette.primaryAccent,
                                            fontSize = 12.sp,
                                            fontWeight = FontWeight.Medium,
                                        )
                                    }
                                    Text(
                                        text = botDescription,
                                        color = palette.secondaryText,
                                        fontSize = 13.sp,
                                        lineHeight = 19.sp,
                                    )
                                }
                            }
                        }
                    } else {
                        Box(
                            modifier = Modifier.fillMaxSize().padding(32.dp),
                            contentAlignment = Alignment.Center,
                        ) {
                            Surface(
                                shape = RoundedCornerShape(14.dp),
                                color = palette.servicePillBg,
                            ) {
                                Text(
                                    text = strings.noMessagesYet,
                                    color = palette.servicePillFg,
                                    fontSize = 13.sp,
                                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                                )
                            }
                        }
                    }
                } else {
                    LazyColumn(
                        state = listState,
                        modifier = Modifier
                            .fillMaxSize()
                            .testTag("chat-message-list"),
                        contentPadding = PaddingValues(horizontal = 12.dp, vertical = 10.dp),
                    ) {
                        if (showLoadOlderHeader) {
                            item(key = "load-older-header") {
                                Box(
                                    modifier = Modifier
                                        .fillMaxWidth()
                                        .padding(vertical = 6.dp),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Surface(
                                        shape = RoundedCornerShape(14.dp),
                                        color = palette.servicePillBg,
                                        modifier = Modifier
                                            .clip(RoundedCornerShape(14.dp))
                                            .clickable(onClick = onLoadOlderMessages),
                                    ) {
                                        Text(
                                            text = strings.loadOlderMessages,
                                            color = palette.primaryAccent,
                                            fontSize = 12.sp,
                                            fontWeight = FontWeight.Medium,
                                            modifier = Modifier.padding(horizontal = 14.dp, vertical = 6.dp),
                                        )
                                    }
                                }
                            }
                        }

                        itemsIndexed(
                            items = timelineItems,
                            key = { idx, entry -> timelineItemKey(idx, entry) },
                        ) { _, entry ->
                            when (entry) {
                                is ChatTimelineItem.DateDivider -> {
                                    Box(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .padding(vertical = 10.dp),
                                        contentAlignment = Alignment.Center,
                                    ) {
                                        Surface(
                                            shape = RoundedCornerShape(12.dp),
                                            color = palette.servicePillBg,
                                        ) {
                                            Text(
                                                text = entry.label,
                                                color = palette.servicePillFg,
                                                fontSize = 11.sp,
                                                fontWeight = FontWeight.Medium,
                                                modifier = Modifier.padding(horizontal = 11.dp, vertical = 4.dp),
                                            )
                                        }
                                    }
                                }

                                is ChatTimelineItem.MessageEntry -> {
                                    val msg = entry.message
                                    if (msg.contentType == "service") {
                                        Box(
                                            modifier = Modifier
                                                .fillMaxWidth()
                                                .padding(vertical = 6.dp),
                                            contentAlignment = Alignment.Center,
                                        ) {
                                            Surface(
                                                shape = RoundedCornerShape(12.dp),
                                                color = palette.servicePillBg,
                                            ) {
                                                Text(
                                                    text = msg.text,
                                                    color = palette.servicePillFg,
                                                    fontSize = 12.sp,
                                                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 5.dp),
                                                )
                                            }
                                        }
                                    } else {
                                        AppleMessageRow(
                                            chat = chat,
                                            message = msg,
                                            groupedPrev = entry.isGroupedWithPrev,
                                            groupedNext = entry.isGroupedWithNext,
                                            audioPlayback = state.audioPlayback,
                                            onLongClick = { contextMenuMessage = msg },
                                            onOpenMedia = { onOpenMediaViewer(msg) },
                                            onToggleReaction = { emoji -> onToggleReaction(msg.id, emoji) },
                                            onVotePoll = { optId -> onVotePoll(msg.id, optId) },
                                            onPlayAudio = { frac -> onPlayOrToggleAudio(msg, frac) },
                                            onClickInlineButton = { btn -> onClickInlineButton(msg.id, btn) },
                                            onResolveMikaConfirmation = { approved ->
                                                onResolveMikaConfirmation(approved, msg.mikaTaskId ?: chat.id, msg.mikaAskId)
                                            },
                                            onRetryFailed = { onRetryFailedMessage(msg.id) },
                                            onOpenDiscussion = { onOpenDiscussion(msg) },
                                            onCopyCode = { code ->
                                                val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                                                cm?.setPrimaryClip(ClipData.newPlainText("code", code))
                                                onToast(strings.copied)
                                            },
                                        )
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // 4. Desktop v1.6.9 Pre-Start Bot Bar OR Floating Apple Capsule Composer
            if (isUnstartedBot) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .background(palette.appBackground)
                        .navigationBarsPadding(),
                ) {
                    TelegramHairlineDivider()
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(54.dp)
                            .clickable(onClick = onStartBotChat)
                            .testTag("bot-start-bar"),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            text = strings.botStartAction,
                            color = palette.primaryAccent,
                            fontSize = 16.sp,
                            fontWeight = FontWeight.SemiBold,
                            letterSpacing = 0.6.sp,
                        )
                    }
                }
            } else {
                TelegramComposer(
                    chat = chat,
                    draftText = chat.draftText,
                    replyTarget = state.replyTarget,
                    editTarget = state.editTarget,
                    recordingState = state.recordingState,
                    slashCommands = state.slashCommands,
                    stickerPacks = state.stickerPacks,
                    gifCatalog = state.gifCatalog,
                    onDraftChange = onUpdateDraft,
                    onClearReplyOrEdit = {
                        onSetReplyTarget(null)
                        onSetEditTarget(null)
                    },
                    onSendMessage = onSendMessage,
                    onSendSticker = onSendSticker,
                    onSendGif = onSendGif,
                    onOpenAttachmentSheet = { onSetAttachmentSheetOpen(true) },
                    onOpenMiniApp = onOpenMiniApp,
                    onToggleRecordMode = onToggleRecordMode,
                    onStartRecording = onStartRecording,
                    onPermissionDenied = onToast,
                    onDragRecording = onDragRecording,
                    onCancelRecording = onCancelRecording,
                    onFinishRecording = onFinishRecording,
                )
            }
        }

        // ====================================================================
        // Root-Level Overlays (never inside the Column!)
        // ====================================================================

        // 1. Hoisted Root Attachment Sheet
        TelegramAttachmentRootOverlay(
            visible = state.composerAttachmentSheetOpen,
            onDismiss = { onSetAttachmentSheetOpen(false) },
            onPickPhotoOrFile = { mediaPickerLauncher.launch(arrayOf("*/*")) },
            onOpenPollCreator = { showPollCreator = true },
            onOpenContactDialog = { showContactDialog = true },
            onRequestLocation = {
                val fine = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
                val coarse = ContextCompat.checkSelfPermission(context, Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
                if (fine || coarse) {
                    val coords = fetchRealDeviceLocationOrNull(context)
                    if (coords != null) {
                        onSendLocation(coords.first, coords.second, if (strings.isEnglish) "Current Location" else "当前位置")
                    } else {
                        onToast(if (strings.isEnglish) "Unable to acquire GPS fix." else "暂未获取到有效定位，请确认已开启定位服务")
                    }
                } else {
                    locationPermLauncher.launch(
                        arrayOf(Manifest.permission.ACCESS_FINE_LOCATION, Manifest.permission.ACCESS_COARSE_LOCATION),
                    )
                }
            },
            onRequestQuickVoice = {
                val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED
                if (granted) onStartRecording() else voicePermLauncher.launch(Manifest.permission.RECORD_AUDIO)
            },
        )

        // 2. Desktop v1.6.6 Muse 4-Tab Peer/Bot Inspector
        MuseInspectorSheet(
            visible = showMuseInspector,
            chat = chat,
            messages = messages,
            fabushiBot = matchingFabushiBot,
            computers = state.cloudComputer.nodes,
            boundNodeId = state.botComputerBindings[botIdKey] ?: matchingFabushiBot?.boundNodeId,
            onDismiss = { showMuseInspector = false },
            onToggleMute = onToggleMuteChat,
            onTogglePin = onTogglePinChat,
            onSetTtl = onSetChatTtl,
            onClearHistory = onClearChatHistory,
            onLeaveChat = onLeaveChat,
            onOpenSecretVerify = onOpenSecretVerify,
            onOpenChannelStats = onOpenChannelStats,
            onOpenMediaViewer = onOpenMediaViewer,
            onSendQuickCommand = { cmd -> onSendMessage(cmd, false, null, null) },
            onPatchBot = onPatchFabushiBot,
            onBindBotComputer = onBindBotComputer,
            onToast = onToast,
        )

        // 3. Message Long-Press Context Menu + Quick Tapback Strip
        contextMenuMessage?.let { msg ->
            TelegramBottomSheet(visible = true, onDismiss = { contextMenuMessage = null }) {
                Column(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 16.dp, vertical = 6.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    // Quick Tapbacks strip
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceEvenly,
                    ) {
                        listOf("❤️", "👍", "👎", "😂", "‼️", "❓").forEach { emoji ->
                            Box(
                                modifier = Modifier
                                    .size(40.dp)
                                    .clip(CircleShape)
                                    .background(palette.inputBg)
                                    .clickable {
                                        onToggleReaction(msg.id, emoji)
                                        contextMenuMessage = null
                                    },
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(emoji, fontSize = 18.sp)
                            }
                        }
                    }
                    TelegramHairlineDivider()
                    ContextMenuRow(TelegramGlyph.REPLY_ARROW, if (strings.isEnglish) "Reply" else "回复") {
                        onSetReplyTarget(msg)
                        contextMenuMessage = null
                    }
                    if (msg.outgoing && msg.text.isNotBlank()) {
                        ContextMenuRow(TelegramGlyph.PENCIL, if (strings.isEnglish) "Edit" else "编辑") {
                            onSetEditTarget(msg)
                            contextMenuMessage = null
                        }
                    }
                    ContextMenuRow(TelegramGlyph.PIN, if (msg.pinned) strings.unpinAction else strings.pinAction) {
                        onTogglePinMessage(msg.id)
                        contextMenuMessage = null
                    }
                    if (msg.text.isNotBlank()) {
                        ContextMenuRow(TelegramGlyph.COPY, strings.copy) {
                            val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager
                            cm?.setPrimaryClip(ClipData.newPlainText("message", msg.text))
                            onToast(strings.copied)
                            contextMenuMessage = null
                        }
                        if (state.showTranslateButton) {
                            ContextMenuRow(TelegramGlyph.TRANSLATE, if (strings.isEnglish) "Translate" else "翻译") {
                                onTranslateMessage(msg.id)
                                contextMenuMessage = null
                            }
                        }
                    }
                    ContextMenuRow(TelegramGlyph.FORWARD_ARROW, if (strings.isEnglish) "Forward" else "转发") {
                        forwardMessageTarget = msg
                        contextMenuMessage = null
                    }
                    ContextMenuRow(
                        glyph = TelegramGlyph.TRASH,
                        label = strings.delete,
                        tint = palette.destructiveRed,
                    ) {
                        onDeleteMessage(msg.id, true)
                        contextMenuMessage = null
                    }
                }
            }
        }

        // 4. Forward Target Picker Sheet
        forwardMessageTarget?.let { msg ->
            TelegramBottomSheet(visible = true, onDismiss = { forwardMessageTarget = null }) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    Text(if (strings.isEnglish) "Forward to..." else "转发给…", color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                    state.chats.take(10).forEach { target ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(10.dp))
                                .clickable {
                                    onForwardMessage(msg, target.id)
                                    forwardMessageTarget = null
                                }
                                .padding(vertical = 8.dp, horizontal = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            TelegramAvatar(idSeed = target.id, title = target.title, size = 36.dp, kind = target.kind)
                            Spacer(Modifier.width(10.dp))
                            Text(target.title, color = palette.primaryText, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }

        // 5. Channel Statistics Sheet (guarded against empty dailyViewsBars!)
        state.channelStatsSnapshot?.let { stats ->
            TelegramBottomSheet(visible = true, onDismiss = onCloseChannelStats) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    Text("频道统计 · ${chat.title}", color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        Column {
                            Text("${stats.subscribers}", color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 18.sp)
                            Text("订阅者", color = palette.secondaryText, fontSize = 11.sp)
                        }
                        Column {
                            Text("${stats.avgViewsPerPost}", color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 18.sp)
                            Text("平均阅读", color = palette.secondaryText, fontSize = 11.sp)
                        }
                        Column {
                            Text("${stats.reactionsPerPost}", color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 18.sp)
                            Text("篇均互动", color = palette.secondaryText, fontSize = 11.sp)
                        }
                    }
                    if (stats.dailyViewsBars.isNotEmpty()) {
                        val maxBar = (stats.dailyViewsBars.maxOrNull() ?: 1).coerceAtLeast(1)
                        Canvas(modifier = Modifier.fillMaxWidth().height(90.dp)) {
                            val barW = size.width / (stats.dailyViewsBars.size * 1.8f)
                            val gap = barW * 0.8f
                            stats.dailyViewsBars.forEachIndexed { idx, value ->
                                val h = (value.toFloat() / maxBar.toFloat()) * size.height * 0.9f
                                drawRoundRect(
                                    color = palette.primaryAccent,
                                    topLeft = Offset(idx * (barW + gap), size.height - h),
                                    size = Size(barW, h),
                                    cornerRadius = CornerRadius(6f, 6f),
                                )
                            }
                        }
                    } else {
                        Text("暂无历史浏览趋势数据", color = palette.secondaryText, fontSize = 12.sp)
                    }
                }
            }
        }

        // 6. Channel Discussion Sheet
        state.discussionSheetMessage?.let { rootMsg ->
            val comments = state.discussionCommentsByMessage[rootMsg.id].orEmpty()
            var commentDraft by remember(rootMsg.id) { mutableStateOf("") }
            TelegramBottomSheet(visible = true, onDismiss = { onOpenDiscussion(null) }) {
                Column(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text("频道评论 (${comments.size})", color = palette.primaryText, fontWeight = FontWeight.Bold, fontSize = 16.sp)
                    if (comments.isEmpty()) {
                        Text("暂无评论", color = palette.secondaryText, fontSize = 13.sp)
                    } else {
                        comments.takeLast(8).forEach { c ->
                            Surface(shape = RoundedCornerShape(10.dp), color = palette.cardBg, modifier = Modifier.fillMaxWidth()) {
                                Column(modifier = Modifier.padding(10.dp)) {
                                    Text(c.senderName, color = palette.primaryAccent, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                                    Text(c.text, color = palette.primaryText, fontSize = 13.sp)
                                }
                            }
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Box(
                            modifier = Modifier
                                .weight(1f)
                                .clip(RoundedCornerShape(16.dp))
                                .background(palette.inputBg)
                                .padding(horizontal = 12.dp, vertical = 9.dp),
                        ) {
                            if (commentDraft.isEmpty()) Text("写下评论…", color = palette.hintText, fontSize = 14.sp)
                            BasicTextField(
                                value = commentDraft,
                                onValueChange = { commentDraft = it },
                                textStyle = TextStyle(color = palette.primaryText, fontSize = 14.sp),
                                cursorBrush = SolidColor(palette.searchCursorColor),
                                modifier = Modifier.fillMaxWidth(),
                            )
                        }
                        Button(
                            enabled = commentDraft.isNotBlank(),
                            onClick = {
                                onPostDiscussionComment(rootMsg, commentDraft.trim())
                                commentDraft = ""
                            },
                            colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                        ) {
                            Text("发送")
                        }
                    }
                }
            }
        }

        // 7. Hoisted Poll Creator & Contact Share Dialogs
        if (showPollCreator) {
            TelegramPollCreatorDialog(
                onDismiss = { showPollCreator = false },
                onCreatePoll = onSendPoll,
            )
        }
        if (showContactDialog) {
            TelegramContactShareDialog(
                onDismiss = { showContactDialog = false },
                onSendContact = onSendContact,
            )
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun AppleMessageRow(
    chat: TelegramChatItem,
    message: TelegramMessageItem,
    groupedPrev: Boolean,
    groupedNext: Boolean,
    audioPlayback: FloatingAudioPlaybackState,
    onLongClick: () -> Unit,
    onOpenMedia: () -> Unit,
    onToggleReaction: (String) -> Unit,
    onVotePoll: (String) -> Unit,
    onPlayAudio: (Float?) -> Unit,
    onClickInlineButton: (InlineKeyboardButtonModel) -> Unit,
    onResolveMikaConfirmation: (Boolean) -> Unit,
    onRetryFailed: () -> Unit,
    onOpenDiscussion: () -> Unit,
    onCopyCode: (String) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val outgoing = message.outgoing
    val r = palette.settings.boundedCornerDp
    val innerR = 6.dp
    val bubbleShape = if (outgoing) {
        RoundedCornerShape(
            topStart = r,
            topEnd = if (groupedPrev) innerR else r,
            bottomStart = r,
            bottomEnd = if (groupedNext) innerR else r,
        )
    } else {
        RoundedCornerShape(
            topStart = if (groupedPrev) innerR else r,
            topEnd = r,
            bottomStart = if (groupedNext) innerR else r,
            bottomEnd = r,
        )
    }

    val textColor = if (outgoing) palette.outgoingText else palette.incomingText
    val metaColor = if (outgoing) palette.outgoingMeta else palette.incomingMeta
    val linkColor = if (outgoing) palette.outgoingLinkColor else palette.linkColor
    val replyBarColor = if (outgoing) palette.outgoingReplyBarColor else palette.primaryAccent
    val codeBg = if (outgoing) palette.outgoingCodeBg else palette.codeBackground
    val codeFg = if (outgoing) palette.outgoingText else palette.codeForeground

    val topPadding = if (groupedPrev) 2.dp else 7.dp
    val showGroupAvatar = !outgoing && (chat.kind == ConversationKind.GROUP || chat.kind == ConversationKind.CHANNEL)

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = topPadding, bottom = if (groupedNext) 1.dp else 4.dp),
        horizontalAlignment = if (outgoing) Alignment.End else Alignment.Start,
    ) {
        // Sender name for group incoming first-in-cluster messages
        if (showGroupAvatar && !groupedPrev && message.senderName.isNotBlank()) {
            Text(
                text = message.senderName,
                color = resolvePeerNameColor(message.senderId.ifBlank { message.senderName }, palette),
                fontSize = 11.sp,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.padding(start = 38.dp, bottom = 2.dp),
            )
        }

        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = if (outgoing) Arrangement.End else Arrangement.Start,
            verticalAlignment = Alignment.Bottom,
        ) {
            if (showGroupAvatar) {
                if (!groupedNext) {
                    TelegramAvatar(
                        idSeed = message.senderId.ifBlank { message.senderName },
                        title = message.senderName.ifBlank { chat.title },
                        size = 28.dp,
                    )
                } else {
                    Spacer(Modifier.width(28.dp))
                }
                Spacer(Modifier.width(6.dp))
            }

            // Bubble + Floating 26dp Tapbacks wrapper
            Box(modifier = Modifier.widthIn(min = 44.dp, max = 304.dp)) {
                val outgoingBotHue = if (outgoing && chat.isBot && chat.kind == ConversationKind.DIRECT) {
                    chat.avatarColor?.takeIf { isTintedBotHue(it) }?.let { resolveBotHueColors(it, palette.isDark) }
                } else {
                    null
                }
                val resolvedOutgoingBrush = outgoingBotHue?.let {
                    Brush.verticalGradient(listOf(it.bubbleTop, it.bubbleFill))
                } ?: palette.outgoingBubbleBrush
                val bubbleModifier = Modifier
                    .padding(top = if (message.reactions.isNotEmpty()) 10.dp else 0.dp)
                    .clip(bubbleShape)
                    .let { base ->
                        if (outgoing) base.background(resolvedOutgoingBrush)
                        else base.background(palette.incomingBubble)
                    }
                    .combinedClickable(
                        onClick = {
                            if (message.mediaBlobId != null || message.contentType in setOf("photo", "video", "file", "document")) {
                                onOpenMedia()
                            } else if (message.deliveryState == "failed") {
                                onRetryFailed()
                            }
                        },
                        onLongClick = onLongClick,
                    )

                Column(
                    modifier = bubbleModifier.padding(horizontal = 13.dp, vertical = 9.dp),
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    // Forwarded Header (contrast-safe on outgoing!)
                    message.forwardHeader?.let { fwd ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            TelegramIcon(
                                glyph = TelegramGlyph.FORWARD_ARROW,
                                tint = if (outgoing) palette.outgoingLinkColor else palette.primaryAccent,
                                size = 12.dp,
                            )
                            Spacer(Modifier.width(4.dp))
                            Text(
                                text = (if (strings.isEnglish) "Forwarded from " else "转发自 ") + fwd.originName,
                                color = if (outgoing) palette.outgoingMeta else palette.primaryAccent,
                                fontSize = 11.sp,
                                fontWeight = FontWeight.Medium,
                            )
                        }
                    }

                    // Reply Quote Box (contrast-safe dark-tinted sub-card on outgoing!)
                    message.replyHeader?.let { rep ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(8.dp))
                                .background(if (outgoing) palette.outgoingSubCardBg else palette.primaryAccent.copy(alpha = 0.10f))
                                .padding(horizontal = 8.dp, vertical = 5.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(
                                modifier = Modifier
                                    .width(2.5.dp)
                                    .height(26.dp)
                                    .clip(RoundedCornerShape(2.dp))
                                    .background(replyBarColor),
                            )
                            Spacer(Modifier.width(8.dp))
                            Column {
                                Text(
                                    text = rep.senderName,
                                    color = if (outgoing) palette.outgoingText else palette.primaryAccent,
                                    fontSize = 11.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    maxLines = 1,
                                )
                                Text(
                                    text = rep.quoteText,
                                    color = if (outgoing) palette.outgoingMeta else palette.secondaryText,
                                    fontSize = 11.sp,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                    }

                    // Specialized Content Bodies
                    when {
                        message.contentType == "sticker" && !message.stickerEmoji.isNullOrBlank() -> {
                            Text(message.stickerEmoji, fontSize = 52.sp)
                        }

                        message.poll != null -> {
                            ApplePollBody(
                                poll = message.poll,
                                outgoing = outgoing,
                                textColor = textColor,
                                metaColor = metaColor,
                                onVote = onVotePoll,
                            )
                        }

                        message.contentType == "voice" -> {
                            AppleVoiceBody(
                                message = message,
                                outgoing = outgoing,
                                isPlaying = audioPlayback.active && audioPlayback.messageId == message.id && audioPlayback.isPlaying,
                                progress = if (audioPlayback.active && audioPlayback.messageId == message.id) audioPlayback.progressFraction else 0f,
                                textColor = textColor,
                                metaColor = metaColor,
                                onTogglePlay = { onPlayAudio(null) },
                            )
                        }

                        message.contentType == "videoNote" -> {
                            val progress = if (audioPlayback.active && audioPlayback.messageId == message.id) audioPlayback.progressFraction else 0f
                            Box(
                                modifier = Modifier
                                    .size(160.dp)
                                    .clip(CircleShape)
                                    .background(Color.Black.copy(alpha = 0.25f))
                                    .clickable(onClick = onOpenMedia),
                                contentAlignment = Alignment.Center,
                            ) {
                                TelegramIcon(TelegramGlyph.PLAY, tint = Color.White, size = 34.dp)
                                if (progress > 0f) {
                                    Canvas(modifier = Modifier.fillMaxSize()) {
                                        drawArc(
                                            color = Color.White,
                                            startAngle = -90f,
                                            sweepAngle = 360f * progress,
                                            useCenter = false,
                                            style = androidx.compose.ui.graphics.drawscope.Stroke(width = 3.dp.toPx()),
                                        )
                                    }
                                }
                            }
                        }

                        message.contentType in setOf("photo", "video", "file", "document") || message.mediaBlobId != null || !message.mediaThumbnailDataUrl.isNullOrBlank() -> {
                            AppleMediaCard(
                                message = message,
                                outgoing = outgoing,
                                textColor = textColor,
                                metaColor = metaColor,
                                onOpen = onOpenMedia,
                            )
                        }

                        message.contentType == "location" -> {
                            val lat = message.latitude
                            val lon = message.longitude
                            Row(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(10.dp))
                                    .background(if (outgoing) palette.outgoingSubCardBg else palette.inputBg)
                                    .padding(10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                TelegramIcon(TelegramGlyph.LOCATION_PIN, tint = if (outgoing) palette.outgoingLinkColor else palette.primaryAccent, size = 20.dp)
                                Spacer(Modifier.width(8.dp))
                                Column {
                                    Text(message.venueTitle ?: "共享位置", color = textColor, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                                    Text(
                                        text = if (lat != null && lon != null) "%.4f, %.4f".format(lat, lon) else "位置信息不可用",
                                        color = metaColor,
                                        fontSize = 11.sp,
                                    )
                                }
                            }
                        }

                        message.contentType == "contact" -> {
                            Row(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(10.dp))
                                    .background(if (outgoing) palette.outgoingSubCardBg else palette.inputBg)
                                    .padding(10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                TelegramIcon(TelegramGlyph.CONTACTS, tint = if (outgoing) palette.outgoingLinkColor else palette.primaryAccent, size = 20.dp)
                                Spacer(Modifier.width(8.dp))
                                Column {
                                    Text(message.contactName ?: "联系人", color = textColor, fontWeight = FontWeight.SemiBold, fontSize = 13.sp)
                                    if (!message.contactPhone.isNullOrBlank()) {
                                        Text(message.contactPhone, color = if (outgoing) palette.outgoingLinkColor else palette.primaryAccent, fontSize = 12.sp)
                                    }
                                }
                            }
                        }
                    }

                    // Deduplicated Text + Rich UTF-16 Entities + Copyable Fenced Code Blocks
                    if (message.text.isNotBlank()) {
                        val uriHandler = LocalUriHandler.current
                        val hasSpoiler = remember(message.entities) {
                            message.entities.any { it.type == MessageEntityType.SPOILER }
                        }
                        var spoilersRevealed by remember(message.id) { mutableStateOf(false) }
                        val segments = remember(message.text) { parseMessageCodeSegments(message.text) }
                        var segmentSearchCursor = 0
                        segments.forEach { seg ->
                            when (seg) {
                                is MessageContentSegment.TextSegment -> {
                                    val segStartInFull = message.text.indexOf(seg.text, segmentSearchCursor)
                                        .takeIf { it >= 0 } ?: 0
                                    segmentSearchCursor = (segStartInFull + seg.text.length).coerceAtMost(message.text.length)
                                    val segmentEntities = remember(message.entities, segStartInFull, seg.text.length) {
                                        if (message.entities.isEmpty()) {
                                            emptyList()
                                        } else {
                                            val segEnd = segStartInFull + seg.text.length
                                            message.entities.mapNotNull { entity ->
                                                val overlapStart = maxOf(entity.offset, segStartInFull)
                                                val overlapEnd = minOf(entity.endOffset, segEnd)
                                                if (overlapEnd > overlapStart) {
                                                    entity.copy(
                                                        offset = overlapStart - segStartInFull,
                                                        length = overlapEnd - overlapStart,
                                                    )
                                                } else null
                                            }
                                        }
                                    }
                                    val annotated = remember(seg.text, segmentEntities, linkColor, codeBg, textColor, metaColor, spoilersRevealed) {
                                        buildRichMessageAnnotatedString(
                                            text = seg.text,
                                            entities = segmentEntities,
                                            linkColor = linkColor,
                                            codeBg = codeBg,
                                            textColor = textColor,
                                            spoilerMaskColor = metaColor.copy(alpha = 0.45f),
                                            revealSpoilers = spoilersRevealed,
                                            onOpenUrl = { url -> runCatching { uriHandler.openUri(url) } },
                                        )
                                    }
                                    Text(
                                        text = annotated,
                                        color = textColor,
                                        fontSize = palette.settings.boundedFontSp,
                                        lineHeight = (palette.settings.messageFontSizeSp.coerceIn(12, 22) + 5).sp,
                                        modifier = if (hasSpoiler && !spoilersRevealed) {
                                            Modifier.clickable { spoilersRevealed = true }
                                        } else {
                                            Modifier
                                        },
                                    )
                                }

                                is MessageContentSegment.CodeBlockSegment -> {
                                    Surface(
                                        shape = RoundedCornerShape(10.dp),
                                        color = codeBg,
                                        modifier = Modifier.fillMaxWidth(),
                                    ) {
                                        Column(modifier = Modifier.padding(10.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                                            Row(
                                                modifier = Modifier.fillMaxWidth(),
                                                horizontalArrangement = Arrangement.SpaceBetween,
                                                verticalAlignment = Alignment.CenterVertically,
                                            ) {
                                                Text(
                                                    text = seg.language.ifBlank { "code" },
                                                    color = linkColor,
                                                    fontFamily = FontFamily.Monospace,
                                                    fontSize = 11.sp,
                                                    fontWeight = FontWeight.SemiBold,
                                                )
                                                Text(
                                                    text = strings.copyCode,
                                                    color = linkColor,
                                                    fontSize = 11.sp,
                                                    fontWeight = FontWeight.SemiBold,
                                                    modifier = Modifier
                                                        .clip(RoundedCornerShape(6.dp))
                                                        .clickable { onCopyCode(seg.code) }
                                                        .padding(horizontal = 4.dp, vertical = 2.dp),
                                                )
                                            }
                                            Text(
                                                text = seg.code,
                                                color = codeFg,
                                                fontFamily = FontFamily.Monospace,
                                                fontSize = 12.sp,
                                                lineHeight = 17.sp,
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Translated Text Callout
                    if (!message.translatedText.isNullOrBlank()) {
                        Surface(
                            shape = RoundedCornerShape(8.dp),
                            color = if (outgoing) palette.outgoingSubCardBg else palette.primaryAccent.copy(alpha = 0.10f),
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text(
                                text = message.translatedText,
                                color = textColor,
                                fontSize = 13.sp,
                                modifier = Modifier.padding(8.dp),
                            )
                        }
                    }

                    // Mika Live Task Status & Inline ConfirmationGate Approve / Decline Buttons
                    if (message.mikaTaskId != null || message.mikaRequiresConfirmation || message.mikaAskId != null) {
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = if (outgoing) palette.outgoingSubCardBg else palette.cardBg,
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Column(
                                modifier = Modifier.padding(10.dp),
                                verticalArrangement = Arrangement.spacedBy(6.dp),
                            ) {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                    TelegramIcon(TelegramGlyph.MIKA_PHONE, tint = if (outgoing) palette.outgoingLinkColor else palette.primaryAccent, size = 14.dp)
                                    Text(
                                        text = message.mikaTaskStatus?.ifBlank { "Mika Task" } ?: "Mika",
                                        color = textColor,
                                        fontSize = 12.sp,
                                        fontWeight = FontWeight.SemiBold,
                                    )
                                }
                                message.mikaStepLogs.takeLast(4).forEach { step ->
                                    Text("• $step", color = metaColor, fontSize = 11.sp)
                                }
                                // Strictly gate Approve/Decline on mikaRequiresConfirmation (never ASK_INPUT / ASK_TAKEOVER)
                                if (message.mikaRequiresConfirmation) {
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                                    ) {
                                        Button(
                                            onClick = { onResolveMikaConfirmation(true) },
                                            colors = ButtonDefaults.buttonColors(
                                                containerColor = palette.approveActionBg,
                                                contentColor = palette.approveActionContent,
                                            ),
                                            shape = RoundedCornerShape(8.dp),
                                            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
                                            modifier = Modifier.weight(1f).testTag("mika-confirm-approve-${message.id}"),
                                        ) {
                                            Text(strings.approve, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                                        }
                                        OutlinedButton(
                                            onClick = { onResolveMikaConfirmation(false) },
                                            shape = RoundedCornerShape(8.dp),
                                            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 4.dp),
                                            modifier = Modifier.weight(1f).testTag("mika-confirm-decline-${message.id}"),
                                        ) {
                                            Text(
                                                strings.decline,
                                                color = if (outgoing) palette.outgoingText else palette.destructiveRed,
                                                fontSize = 12.sp,
                                                fontWeight = FontWeight.SemiBold,
                                            )
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Footer: Edited, Timestamp, Delivery Checkmark
                    Row(
                        modifier = Modifier.align(Alignment.End),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        if (message.edited) {
                            Text(if (strings.isEnglish) "edited" else "已编辑", color = metaColor, fontSize = 10.sp)
                        }
                        Text(message.time, color = metaColor, fontSize = 10.sp)
                        if (outgoing) {
                            when (message.deliveryState) {
                                "failed" -> {
                                    TelegramIcon(
                                        TelegramGlyph.RETRY_REFRESH,
                                        tint = if (palette.isAppleStyle) palette.outgoingCheckMarkColor else palette.destructiveRed,
                                        size = 13.dp,
                                    )
                                }
                                "sending" -> {
                                    TelegramIcon(TelegramGlyph.CLOCK_SENDING, tint = palette.outgoingCheckMarkColor, size = 12.dp)
                                }
                                "sent" -> {
                                    TelegramIcon(TelegramGlyph.CHECK_SINGLE, tint = palette.outgoingCheckMarkColor, size = 13.dp)
                                }
                                else -> {
                                    TelegramIcon(TelegramGlyph.CHECK_DOUBLE, tint = palette.outgoingCheckMarkColor, size = 13.dp)
                                }
                            }
                        }
                    }

                    // Channel Comments Link
                    if (chat.kind == ConversationKind.CHANNEL && (message.commentsCount ?: 0) > 0) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable(onClick = onOpenDiscussion)
                                .padding(top = 2.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = "${message.commentsCount} 条评论",
                                color = if (outgoing) palette.outgoingLinkColor else palette.primaryAccent,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                }

                // Floating 26dp Circular Tapback Pills on top bubble corner (2dp stage border!)
                if (message.reactions.isNotEmpty()) {
                    Row(
                        modifier = Modifier
                            .align(if (outgoing) Alignment.TopStart else Alignment.TopEnd)
                            .offset(x = if (outgoing) (-6).dp else 6.dp, y = (-2).dp),
                        horizontalArrangement = Arrangement.spacedBy(3.dp),
                    ) {
                        message.reactions.take(3).forEach { rx ->
                            Box(
                                modifier = Modifier
                                    .height(26.dp)
                                    .clip(RoundedCornerShape(13.dp))
                                    .background(if (rx.chosenByMe) palette.primaryAccent else palette.modalBg)
                                    .border(2.dp, palette.appBackground, RoundedCornerShape(13.dp))
                                    .clickable { onToggleReaction(rx.reaction) }
                                    .padding(horizontal = 7.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    text = if (rx.count > 1) "${rx.reaction} ${rx.count}" else rx.reaction,
                                    color = if (rx.chosenByMe) palette.selectedPillContent else palette.primaryText,
                                    fontSize = 11.sp,
                                    fontWeight = FontWeight.SemiBold,
                                )
                            }
                        }
                    }
                }
            }
        }

        // Inline Bot Keyboard Rows (deduplicates mika_confirm buttons when Mika confirmation card renders inline)
        val displayedInlineKeyboard = remember(message.inlineKeyboard, message.mikaRequiresConfirmation) {
            filterMessageInlineKeyboard(message)
        }
        if (displayedInlineKeyboard.isNotEmpty()) {
            Column(
                modifier = Modifier
                    .padding(top = 4.dp, start = if (showGroupAvatar) 34.dp else 0.dp)
                    .widthIn(max = 304.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                displayedInlineKeyboard.forEach { row ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        row.boundedButtons.forEach { btn ->
                            Surface(
                                shape = RoundedCornerShape(10.dp),
                                color = palette.cardBg,
                                modifier = Modifier
                                    .weight(1f)
                                    .border(0.5.dp, palette.borderColor, RoundedCornerShape(10.dp))
                                    .clip(RoundedCornerShape(10.dp))
                                    .clickable { onClickInlineButton(btn) },
                            ) {
                                Box(
                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 8.dp),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text(
                                        text = btn.text,
                                        color = palette.primaryAccent,
                                        fontSize = 12.sp,
                                        fontWeight = FontWeight.SemiBold,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }

        // Explicit Failed-Message Retry Label below outgoing bubble
        if (outgoing && message.deliveryState == "failed") {
            Row(
                modifier = Modifier
                    .padding(top = 3.dp, end = 4.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .clickable(onClick = onRetryFailed)
                    .padding(horizontal = 6.dp, vertical = 2.dp)
                    .testTag("message-retry-${message.id}"),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                TelegramIcon(TelegramGlyph.RETRY_REFRESH, tint = palette.destructiveRed, size = 12.dp)
                Text(
                    text = strings.failedTapRetry,
                    color = palette.destructiveRed,
                    fontSize = 11.sp,
                    fontWeight = FontWeight.Medium,
                )
            }
        }
    }
}

@Composable
private fun AppleMediaCard(
    message: TelegramMessageItem,
    outgoing: Boolean,
    textColor: Color,
    metaColor: Color,
    onOpen: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val decodedBitmap = remember(message.mediaThumbnailDataUrl) {
        decodeDataUrlBitmap(message.mediaThumbnailDataUrl)
    }

    if (decodedBitmap != null) {
        Image(
            bitmap = decodedBitmap.asImageBitmap(),
            contentDescription = message.mediaFileName ?: "media",
            contentScale = ContentScale.Crop,
            modifier = Modifier
                .fillMaxWidth()
                .height(176.dp)
                .clip(RoundedCornerShape(12.dp))
                .clickable(onClick = onOpen),
        )
    } else {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(12.dp))
                .background(if (outgoing) palette.outgoingSubCardBg else palette.inputBg)
                .clickable(onClick = onOpen)
                .padding(10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                modifier = Modifier
                    .size(38.dp)
                    .clip(CircleShape)
                    .background(if (outgoing) palette.outgoingText else palette.primaryAccent),
                contentAlignment = Alignment.Center,
            ) {
                val glyph = when (message.contentType) {
                    "photo" -> TelegramGlyph.MEDIA_PHOTO
                    "video" -> TelegramGlyph.PLAY
                    else -> TelegramGlyph.DOCUMENT_FILE
                }
                TelegramIcon(
                    glyph = glyph,
                    tint = if (outgoing) palette.outgoingBubbleBottom else Color.White,
                    size = 18.dp,
                )
            }
            Spacer(Modifier.width(10.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    text = message.mediaFileName ?: when (message.contentType) {
                        "photo" -> "图片"
                        "video" -> "视频"
                        else -> "文件"
                    },
                    color = textColor,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (message.mediaSizeBytes > 0) {
                    Text(
                        text = "${message.mediaSizeBytes / 1024} KB",
                        color = metaColor,
                        fontSize = 11.sp,
                    )
                }
            }
        }
    }
}

@Composable
private fun AppleVoiceBody(
    message: TelegramMessageItem,
    outgoing: Boolean,
    isPlaying: Boolean,
    progress: Float,
    textColor: Color,
    metaColor: Color,
    onTogglePlay: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val bars = remember(message.waveformBars) {
        if (message.waveformBars.isNotEmpty()) message.waveformBars.take(28)
        else List(24) { 10 + (it * 7) % 18 }
    }
    Row(
        modifier = Modifier.widthIn(min = 180.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            modifier = Modifier
                .size(34.dp)
                .clip(CircleShape)
                .background(if (outgoing) palette.outgoingText else palette.primaryAccent)
                .clickable(onClick = onTogglePlay),
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(
                glyph = if (isPlaying) TelegramGlyph.PAUSE else TelegramGlyph.PLAY,
                tint = if (outgoing) palette.outgoingBubbleBottom else Color.White,
                size = 16.dp,
            )
        }
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth().height(22.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                bars.forEachIndexed { idx, v ->
                    val frac = idx.toFloat() / bars.size.coerceAtLeast(1)
                    val activeBar = frac <= progress
                    val hDp = (4 + (v.coerceIn(1, 31) / 31f) * 16f).dp
                    Box(
                        modifier = Modifier
                            .weight(1f)
                            .height(hDp)
                            .clip(RoundedCornerShape(2.dp))
                            .background(
                                if (outgoing) {
                                    if (activeBar) palette.outgoingText else palette.outgoingText.copy(alpha = 0.42f)
                                } else {
                                    if (activeBar) palette.primaryAccent else palette.secondaryText.copy(alpha = 0.40f)
                                },
                            ),
                    )
                }
            }
            Text(
                text = "%02d:%02d".format(message.durationSeconds / 60, message.durationSeconds % 60),
                color = metaColor,
                fontSize = 11.sp,
            )
        }
    }
}

@Composable
private fun ApplePollBody(
    poll: TelegramPollModel,
    outgoing: Boolean,
    textColor: Color,
    metaColor: Color,
    onVote: (String) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val accent = if (outgoing) palette.outgoingLinkColor else palette.primaryAccent
    Column(verticalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.widthIn(min = 210.dp)) {
        Text(poll.question, color = textColor, fontWeight = FontWeight.Bold, fontSize = 14.sp)
        poll.options.forEach { opt ->
            val pct = if (poll.totalVotes > 0) ((opt.voterCount * 100) / poll.totalVotes).coerceIn(0, 100) else 0
            Surface(
                shape = RoundedCornerShape(10.dp),
                color = if (outgoing) {
                    if (opt.chosen) palette.outgoingSubCardSelectedBg else palette.outgoingSubCardBg
                } else {
                    palette.inputBg
                },
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(10.dp))
                    .clickable { onVote(opt.id) },
            ) {
                Row(
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(opt.text, color = textColor, fontSize = 13.sp, modifier = Modifier.weight(1f))
                    if (poll.hasVoted) {
                        Text("$pct%", color = accent, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
                    }
                }
            }
        }
        Text("${poll.totalVotes} 票", color = metaColor, fontSize = 11.sp)
    }
}

@Composable
private fun ContextMenuRow(
    glyph: TelegramGlyph,
    label: String,
    tint: Color? = null,
    onClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val color = tint ?: palette.primaryText
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TelegramIcon(glyph = glyph, tint = tint ?: palette.primaryAccent, size = 18.dp)
        Spacer(Modifier.width(12.dp))
        Text(label, color = color, fontSize = 15.sp, fontWeight = FontWeight.Medium)
    }
}

private val INLINE_TOKEN_REGEX = Regex("(https?://[^\\s<>()\\[\\]\"']+|@[A-Za-z0-9_]{3,}|/[A-Za-z0-9_]+|#[A-Za-z0-9_\\u4e00-\\u9fa5]+)")
private val SAFE_HTTP_URL_REGEX = Regex("https?://[^\\s<>()\\[\\]\"']+")

internal fun isSafeHttpUrl(raw: String?): Boolean {
    val trimmed = raw?.trim().orEmpty()
    if (trimmed.isEmpty()) return false
    return trimmed.startsWith("https://", ignoreCase = true) || trimmed.startsWith("http://", ignoreCase = true)
}

internal fun extractFirstSafeHttpUrl(text: String, entities: List<MessageEntity> = emptyList()): String? {
    for (entity in entities) {
        when (entity.type) {
            MessageEntityType.TEXT_URL -> {
                if (isSafeHttpUrl(entity.url)) return entity.url?.trim()
            }
            MessageEntityType.URL -> {
                val slice = TelegramEntitiesEngine.substringSafeUtf16(text, entity.offset, entity.length).trim()
                if (isSafeHttpUrl(slice)) return slice
            }
            else -> Unit
        }
    }
    return SAFE_HTTP_URL_REGEX.find(text)?.value?.trim()?.takeIf(::isSafeHttpUrl)
}

internal fun buildRichMessageAnnotatedString(
    text: String,
    entities: List<MessageEntity>,
    linkColor: Color,
    codeBg: Color,
    textColor: Color,
    spoilerMaskColor: Color,
    revealSpoilers: Boolean = false,
    onOpenUrl: ((String) -> Unit)? = null,
): AnnotatedString = buildAnnotatedString {
    append(text)
    val linkRanges = mutableListOf<IntRange>()
    val codeRanges = mutableListOf<IntRange>()

    for (entity in entities) {
        val (start, end) = TelegramEntitiesEngine.sliceSafeUtf16(text, entity.offset, entity.length)
        if (end <= start) continue
        when (entity.type) {
            MessageEntityType.BOLD -> addStyle(SpanStyle(fontWeight = FontWeight.Bold), start, end)
            MessageEntityType.ITALIC -> addStyle(SpanStyle(fontStyle = FontStyle.Italic), start, end)
            MessageEntityType.UNDERLINE -> addStyle(SpanStyle(textDecoration = TextDecoration.Underline), start, end)
            MessageEntityType.STRIKETHROUGH -> addStyle(SpanStyle(textDecoration = TextDecoration.LineThrough), start, end)
            MessageEntityType.CODE, MessageEntityType.PRE -> {
                codeRanges += start until end
                addStyle(SpanStyle(fontFamily = FontFamily.Monospace, background = codeBg), start, end)
            }
            MessageEntityType.SPOILER -> {
                if (!revealSpoilers) {
                    addStyle(
                        SpanStyle(
                            color = textColor.copy(alpha = 0.08f),
                            background = spoilerMaskColor,
                        ),
                        start,
                        end,
                    )
                }
            }
            MessageEntityType.URL -> {
                val url = text.substring(start, end).trim()
                linkRanges += start until end
                if (isSafeHttpUrl(url)) {
                    addLink(
                        LinkAnnotation.Url(
                            url = url,
                            styles = TextLinkStyles(
                                style = SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline),
                            ),
                            linkInteractionListener = onOpenUrl?.let { cb ->
                                { cb(url) }
                            },
                        ),
                        start,
                        end,
                    )
                } else {
                    addStyle(SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline), start, end)
                }
            }
            MessageEntityType.TEXT_URL -> {
                val url = entity.url?.trim()
                linkRanges += start until end
                if (isSafeHttpUrl(url)) {
                    val safeUrl = url!!
                    addLink(
                        LinkAnnotation.Url(
                            url = safeUrl,
                            styles = TextLinkStyles(
                                style = SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline),
                            ),
                            linkInteractionListener = onOpenUrl?.let { cb ->
                                { cb(safeUrl) }
                            },
                        ),
                        start,
                        end,
                    )
                } else {
                    addStyle(SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline), start, end)
                }
            }
            MessageEntityType.MENTION, MessageEntityType.HASHTAG, MessageEntityType.BOT_COMMAND -> {
                addStyle(SpanStyle(color = linkColor, fontWeight = FontWeight.Medium), start, end)
            }
        }
    }

    // Supplement plain text (outside explicit links & code spans) with clickable auto-detected URLs & tokens
    for (match in INLINE_TOKEN_REGEX.findAll(text)) {
        val start = match.range.first
        val end = match.range.last + 1
        val overlapsCodeOrLink = codeRanges.any { start < it.last + 1 && end > it.first } ||
            linkRanges.any { start < it.last + 1 && end > it.first }
        if (overlapsCodeOrLink) continue
        val token = match.value
        if (isSafeHttpUrl(token)) {
            addLink(
                LinkAnnotation.Url(
                    url = token,
                    styles = TextLinkStyles(
                        style = SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline),
                    ),
                    linkInteractionListener = onOpenUrl?.let { cb ->
                        { cb(token) }
                    },
                ),
                start,
                end,
            )
        } else {
            addStyle(
                SpanStyle(
                    color = linkColor,
                    fontWeight = if (token.startsWith("/") || token.startsWith("@")) FontWeight.Medium else FontWeight.Normal,
                ),
                start,
                end,
            )
        }
    }
}

private fun decodeDataUrlBitmap(dataUrl: String?): android.graphics.Bitmap? {
    val raw = dataUrl?.trim()?.takeIf { it.isNotEmpty() } ?: return null
    return runCatching {
        val b64 = if (raw.contains(",")) raw.substringAfter(",") else raw
        val bytes = Base64.decode(b64, Base64.DEFAULT)
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    }.getOrNull()
}

private fun shareChatExportFile(context: Context, file: File, mimeType: String) {
    runCatching {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.media", file)
        val intent = Intent(Intent.ACTION_SEND).apply {
            type = mimeType
            putExtra(Intent.EXTRA_STREAM, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        context.startActivity(Intent.createChooser(intent, "分享聊天记录"))
    }
}

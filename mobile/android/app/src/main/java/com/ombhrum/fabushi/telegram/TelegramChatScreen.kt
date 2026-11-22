package com.ombhrum.fabushi.telegram

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateMapOf
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
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.FileProvider
import com.ombhrum.fabushi.ConversationKind
import java.io.File
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

private val QuickReactionEmojis = listOf("👍", "❤️", "🔥", "🎉", "⚡", "🙏", "🤔", "👀")

@Composable
fun TelegramChatScreen(
    state: TelegramUiState,
    chat: TelegramChatItem,
    onBack: () -> Unit,
    onStartVoiceCall: () -> Unit,
    onStartVideoCall: (groupCall: Boolean) -> Unit,
    onOpenTalkModeOrb: () -> Unit,
    onSetInChatSearch: (Boolean, String) -> Unit,
    onToggleMuteChat: (String) -> Unit,
    onSetChatTtl: (String, Int) -> Unit,
    onOpenSecretChatVerify: (String) -> Unit,
    onOpenChannelStats: (String?) -> Unit,
    onExportChatHistory: (String, String) -> File?,
    onClearChatHistory: (String) -> Unit,
    onSelectForumTopic: (String?) -> Unit,
    onCreateForumTopic: (String, String) -> Unit,
    onCyclePinnedBanner: (String) -> String?,
    onDraftChange: (String) -> Unit,
    onSetReplyTarget: (TelegramMessageItem?) -> Unit,
    onSetEditTarget: (TelegramMessageItem?) -> Unit,
    onSendMessage: (text: String, silent: Boolean, scheduledAtMs: Long?, ttlSeconds: Int?) -> Unit,
    onToggleRecordMode: () -> Unit,
    onStartRecording: () -> Unit,
    onDragRecording: (Float, Float) -> Unit,
    onCancelRecording: () -> Unit,
    onFinishRecording: () -> Unit,
    onSendSticker: (String, String) -> Unit,
    onSendGif: (GifItemModel) -> Unit,
    onSendPollOrQuiz: (String, List<String>, Boolean, Boolean, Boolean, Int, String?) -> Unit,
    onVotePoll: (String, String) -> Unit,
    onSendLocation: (Double, Double, String) -> Unit,
    onSendContact: (String, String) -> Unit,
    onSendMediaAttachment: (String, String, ByteArray) -> Unit,
    onOpenMediaViewer: (TelegramMessageItem) -> Unit,
    onPlayOrToggleAudio: (TelegramMessageItem, Float?) -> Unit,
    onSeekAudio: (Float) -> Unit,
    onCycleAudioSpeed: () -> Unit,
    onStopAudio: () -> Unit,
    onToggleReaction: (String, String) -> Unit,
    onTogglePinMessage: (String) -> Unit,
    onDeleteMessage: (String, Boolean) -> Unit,
    onForwardMessage: (TelegramMessageItem, String) -> Unit,
    onTranslateMessage: (String) -> Unit,
    onClickInlineButton: (InlineKeyboardButtonModel, (String) -> Unit) -> Unit,
    onRunWithMika: (String) -> Unit,
    onOpenGlobalDharmaMiniApp: () -> Unit,
    onOpenDiscussionSheet: (TelegramMessageItem?) -> Unit,
    onPostDiscussionComment: (TelegramMessageItem, String) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val context = LocalContext.current
    val coroutineScope = rememberCoroutineScope()

    BackHandler(onBack = onBack)

    var showOverflowMenu by remember { mutableStateOf(false) }
    var showTtlDialog by remember { mutableStateOf(false) }
    var showCreateTopicDialog by remember { mutableStateOf(false) }
    var showMikaTaskDialog by remember { mutableStateOf(false) }
    var mikaDialogInitialPrompt by remember { mutableStateOf("") }
    var activeMenuMessage by remember { mutableStateOf<TelegramMessageItem?>(null) }
    var forwardCandidate by remember { mutableStateOf<TelegramMessageItem?>(null) }

    val allMessages = state.messagesByChat[chat.id].orEmpty()
    val topics = state.topicsByChat[chat.id].orEmpty()
    val filteredMessages = remember(allMessages, state.activeTopicId, state.inChatSearchActive, state.inChatSearchQuery) {
        allMessages.filter { msg ->
            val topicOk = state.activeTopicId == null || msg.topicId == null || msg.topicId == state.activeTopicId
            val searchOk = !state.inChatSearchActive || state.inChatSearchQuery.isBlank() ||
                msg.text.contains(state.inChatSearchQuery, ignoreCase = true) ||
                msg.senderName.contains(state.inChatSearchQuery, ignoreCase = true)
            topicOk && searchOk
        }
    }

    val listState = rememberLazyListState()
    LaunchedEffect(chat.id, filteredMessages.size) {
        if (filteredMessages.isNotEmpty()) {
            listState.animateScrollToItem(filteredMessages.lastIndex)
        }
    }

    val pinnedMessages = remember(allMessages, chat.pinnedMessageIds) {
        val byId = allMessages.associateBy { it.id }
        chat.pinnedMessageIds.mapNotNull { byId[it] }.ifEmpty { allMessages.filter { it.pinned } }
    }
    val currentPinIdx = (state.pinnedBannerIndexByChat[chat.id] ?: 0).let { idx ->
        if (pinnedMessages.isEmpty()) 0 else idx % pinnedMessages.size
    }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.surfaceColor)
            .imePadding()
            .testTag("telegram-chat-screen"),
    ) {
        // ====================================================================
        // ====================================================================
        // 1. Desktop v1.6.5 Centered Apple Messages Contact Header / Classic Top Bar
        // ====================================================================
        if (palette.isAppleStyle) {
            val subtitleLine = when {
                !chat.typingLabel.isNullOrBlank() -> chat.typingLabel
                chat.kind == ConversationKind.CHANNEL && chat.subscriberCount > 0 -> "${chat.subscriberCount} 位订阅者"
                chat.kind == ConversationKind.GROUP && chat.memberCount > 0 -> "${chat.memberCount} 位成员${if (chat.isForum) " · 多话题" else ""}"
                chat.online -> "在线"
                else -> ""
            }
            val glassPillBg = if (palette.isDark) palette.elevatedSurface.copy(alpha = 0.90f) else Color.White.copy(alpha = 0.90f)
            val glassPillBorder = if (palette.isDark) Color.White.copy(alpha = 0.14f) else Color.Black.copy(alpha = 0.08f)
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.topBarBackground.copy(alpha = 0.94f))
                    .statusBarsPadding()
                    .testTag("apple-centered-contact-header"),
            ) {
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(78.dp)
                        .padding(horizontal = 10.dp),
                ) {
                    // Left circular glass back button
                    Box(
                        modifier = Modifier
                            .align(Alignment.CenterStart)
                            .size(36.dp)
                            .clip(CircleShape)
                            .background(glassPillBg)
                            .border(0.75.dp, glassPillBorder, CircleShape)
                            .clickable(onClick = onBack)
                            .testTag("telegram-chat-back"),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.BACK, tint = palette.primaryAccent, size = 19.dp)
                    }

                    // Center: 38dp Avatar stacked above overlapping frosted Name Pill (Desktop v1.6.5 stage_bars.rs)
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.Center,
                        modifier = Modifier
                            .align(Alignment.Center)
                            .widthIn(max = 220.dp)
                            .clickable {
                                if (chat.isSecretChat) onOpenSecretChatVerify(chat.id)
                                else if (chat.kind == ConversationKind.CHANNEL) onOpenChannelStats(chat.id)
                            },
                    ) {
                        TelegramAvatar(
                            idSeed = chat.id,
                            title = chat.title,
                            size = 38.dp,
                            kind = chat.kind,
                            online = chat.online,
                            isSecret = chat.isSecretChat,
                            isCompanionGhost = chat.isCompanionGhost,
                        )
                        Surface(
                            shape = RoundedCornerShape(999.dp),
                            color = glassPillBg,
                            modifier = Modifier
                                .offset(y = (-4).dp)
                                .border(0.75.dp, glassPillBorder, RoundedCornerShape(999.dp)),
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 10.dp, vertical = 2.dp),
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(3.dp),
                            ) {
                                if (chat.isSecretChat) {
                                    TelegramIcon(TelegramGlyph.LOCK_CLOSED, tint = palette.secretGreen, size = 11.dp)
                                }
                                Text(
                                    text = chat.title,
                                    color = palette.primaryText,
                                    fontSize = 12.5.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                                Text("›", color = palette.secondaryText, fontSize = 12.sp)
                            }
                        }
                        if (subtitleLine.isNotEmpty()) {
                            Row(
                                verticalAlignment = Alignment.CenterVertically,
                                horizontalArrangement = Arrangement.spacedBy(4.dp),
                            ) {
                                if (chat.online && chat.typingLabel.isNullOrBlank()) {
                                    Box(
                                        modifier = Modifier
                                            .size(5.5.dp)
                                            .clip(CircleShape)
                                            .background(palette.secretGreen),
                                    )
                                }
                                Text(
                                    text = subtitleLine,
                                    color = if (!chat.typingLabel.isNullOrBlank()) palette.primaryAccent else palette.secondaryText,
                                    fontSize = 10.5.sp,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                )
                            }
                        }
                    }

                    // Right floating glass action pill (Call / Video / Search / Overflow)
                    Row(
                        modifier = Modifier
                            .align(Alignment.CenterEnd)
                            .clip(RoundedCornerShape(999.dp))
                            .background(glassPillBg)
                            .border(0.75.dp, glassPillBorder, RoundedCornerShape(999.dp))
                            .padding(horizontal = 4.dp, vertical = 2.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(
                            modifier = Modifier
                                .size(30.dp)
                                .clip(CircleShape)
                                .clickable(onClick = onStartVoiceCall),
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.PHONE_CALL, tint = palette.primaryAccent, size = 16.dp)
                        }
                        Box(
                            modifier = Modifier
                                .size(30.dp)
                                .clip(CircleShape)
                                .clickable {
                                    onStartVideoCall(chat.kind == ConversationKind.GROUP || chat.kind == ConversationKind.CHANNEL)
                                },
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.VIDEO_CALL, tint = palette.primaryAccent, size = 16.dp)
                        }
                        Box(
                            modifier = Modifier
                                .size(30.dp)
                                .clip(CircleShape)
                                .clickable { onSetInChatSearch(!state.inChatSearchActive, "") },
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.SEARCH, tint = palette.secondaryText, size = 15.dp)
                        }
                        Box {
                            Box(
                                modifier = Modifier
                                    .size(30.dp)
                                    .clip(CircleShape)
                                    .clickable { showOverflowMenu = true }
                                    .testTag("telegram-chat-overflow"),
                                contentAlignment = Alignment.Center,
                            ) {
                                TelegramIcon(TelegramGlyph.MORE_VERT, tint = palette.secondaryText, size = 16.dp)
                            }
                            DropdownMenu(expanded = showOverflowMenu, onDismissRequest = { showOverflowMenu = false }) {
                                DropdownMenuItem(
                                    text = { Text("🎙 开启 <60ms 全双工语音球") },
                                    onClick = { showOverflowMenu = false; onOpenTalkModeOrb() },
                                )
                                DropdownMenuItem(
                                    text = { Text(if (chat.isMuted) "取消静音" else "静音此会话") },
                                    onClick = { showOverflowMenu = false; onToggleMuteChat(chat.id) },
                                )
                                DropdownMenuItem(
                                    text = { Text("设置阅后即焚 TTL") },
                                    onClick = { showOverflowMenu = false; showTtlDialog = true },
                                )
                                DropdownMenuItem(
                                    text = { Text("导出聊天记录 (JSON)") },
                                    onClick = { showOverflowMenu = false; onExportChatHistory(chat.id, "json") },
                                )
                                DropdownMenuItem(
                                    text = { Text("清空聊天记录", color = palette.destructiveRed) },
                                    onClick = { showOverflowMenu = false; onClearChatHistory(chat.id) },
                                )
                            }
                        }
                    }
                }
                TelegramHairlineDivider()
            }
        } else {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .background(palette.topBarBackground)
                .statusBarsPadding(),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(56.dp)
                    .padding(horizontal = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .size(42.dp)
                        .clip(CircleShape)
                        .clickable(onClick = onBack)
                        .testTag("telegram-chat-back"),
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.BACK, tint = palette.topBarContent, size = 22.dp)
                }

                TelegramAvatar(
                    idSeed = chat.id,
                    title = chat.title,
                    size = 40.dp,
                    kind = chat.kind,
                    online = chat.online,
                    isSecret = chat.isSecretChat,
                    isCompanionGhost = chat.isCompanionGhost,
                    modifier = Modifier.clickable {
                        if (chat.isSecretChat) onOpenSecretChatVerify(chat.id)
                        else if (chat.kind == ConversationKind.CHANNEL) onOpenChannelStats(chat.id)
                    },
                )

                Spacer(Modifier.width(10.dp))

                Column(
                    modifier = Modifier
                        .weight(1f)
                        .clickable {
                            if (chat.isSecretChat) onOpenSecretChatVerify(chat.id)
                            else if (chat.kind == ConversationKind.CHANNEL) onOpenChannelStats(chat.id)
                        },
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (chat.isSecretChat) {
                            TelegramIcon(TelegramGlyph.LOCK_CLOSED, tint = Color(0xFF7BFF9A), size = 14.dp)
                            Spacer(Modifier.width(4.dp))
                        }
                        Text(
                            text = chat.title,
                            color = palette.topBarContent,
                            fontSize = 16.sp,
                            fontWeight = FontWeight.Bold,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        if (chat.isVerified) {
                            Spacer(Modifier.width(4.dp))
                            TelegramIcon(TelegramGlyph.VERIFIED, tint = Color(0xFF64D2FF), size = 15.dp)
                        }
                        if (chat.isMuted) {
                            Spacer(Modifier.width(4.dp))
                            TelegramIcon(TelegramGlyph.MUTE, tint = palette.topBarSubtitle, size = 14.dp)
                        }
                    }

                    val subtitleLine = when {
                        !chat.typingLabel.isNullOrBlank() -> chat.typingLabel
                        chat.kind == ConversationKind.CHANNEL && chat.subscriberCount > 0 -> "${chat.subscriberCount} 位订阅者"
                        chat.kind == ConversationKind.GROUP && chat.memberCount > 0 -> "${chat.memberCount} 位成员${if (chat.isForum) " · 多话题" else ""}"
                        chat.online -> "在线"
                        else -> chat.subtitle
                    }
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            text = subtitleLine,
                            color = if (!chat.typingLabel.isNullOrBlank()) Color(0xFF8BE8FF) else palette.topBarSubtitle,
                            fontSize = 12.sp,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        if (chat.ttlSeconds > 0) {
                            Spacer(Modifier.width(6.dp))
                            Text(
                                text = "⏱ ${formatTtlShort(chat.ttlSeconds)}",
                                color = Color(0xFFFFE082),
                                fontSize = 11.sp,
                                fontWeight = FontWeight.SemiBold,
                            )
                        }
                    }
                }

                // Voice Call
                Box(
                    modifier = Modifier
                        .size(38.dp)
                        .clip(CircleShape)
                        .clickable(onClick = onStartVoiceCall),
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.PHONE_CALL, tint = palette.topBarContent, size = 20.dp)
                }

                // Video / Group Call
                Box(
                    modifier = Modifier
                        .size(38.dp)
                        .clip(CircleShape)
                        .clickable {
                            onStartVideoCall(chat.kind == ConversationKind.GROUP || chat.kind == ConversationKind.CHANNEL)
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.VIDEO_CALL, tint = palette.topBarContent, size = 20.dp)
                }

                // <60ms Full-Duplex VAD Talk Mode Orb
                Box(
                    modifier = Modifier
                        .size(36.dp)
                        .clip(CircleShape)
                        .clickable(onClick = onOpenTalkModeOrb)
                        .testTag("telegram-chat-talk-orb"),
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.TALK_WAVE, tint = Color(0xFF76FF03), size = 19.dp)
                }

                // In-Chat Search
                Box(
                    modifier = Modifier
                        .size(36.dp)
                        .clip(CircleShape)
                        .clickable { onSetInChatSearch(!state.inChatSearchActive, "") },
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.SEARCH, tint = palette.topBarContent, size = 19.dp)
                }

                // 3-Dot Overflow Menu
                Box {
                    Box(
                        modifier = Modifier
                            .size(36.dp)
                            .clip(CircleShape)
                            .clickable { showOverflowMenu = true }
                            .testTag("telegram-chat-overflow"),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.MORE_VERT, tint = palette.topBarContent, size = 20.dp)
                    }

                    DropdownMenu(
                        expanded = showOverflowMenu,
                        onDismissRequest = { showOverflowMenu = false },
                    ) {
                        DropdownMenuItem(
                            text = { Text(if (chat.isMuted) "🔔 取消静音通知" else "🔕 静音会话通知") },
                            onClick = {
                                showOverflowMenu = false
                                onToggleMuteChat(chat.id)
                            },
                        )
                        DropdownMenuItem(
                            text = { Text("⏱ 自动销毁计时器 (${formatTtlShort(chat.ttlSeconds)})") },
                            onClick = {
                                showOverflowMenu = false
                                showTtlDialog = true
                            },
                        )
                        if (chat.isSecretChat) {
                            DropdownMenuItem(
                                text = { Text("🔐 核验端到端加密指纹 (4-Emoji)") },
                                onClick = {
                                    showOverflowMenu = false
                                    onOpenSecretChatVerify(chat.id)
                                },
                            )
                        }
                        if (chat.kind == ConversationKind.CHANNEL) {
                            DropdownMenuItem(
                                text = { Text("📊 频道数据统计看板") },
                                onClick = {
                                    showOverflowMenu = false
                                    onOpenChannelStats(chat.id)
                                },
                            )
                        }
                        DropdownMenuItem(
                            text = { Text("🤖 交由 Mika 手机智能体执行") },
                            onClick = {
                                showOverflowMenu = false
                                mikaDialogInitialPrompt = "根据「${chat.title}」最新聊天内容执行手机操作"
                                showMikaTaskDialog = true
                            },
                        )
                        DropdownMenuItem(
                            text = { Text("📤 导出聊天记录 (JSON)") },
                            onClick = {
                                showOverflowMenu = false
                                onExportChatHistory(chat.id, "json")?.let { shareExportedFile(context, it, "application/json") }
                            },
                        )
                        DropdownMenuItem(
                            text = { Text("📄 导出聊天记录 (HTML)") },
                            onClick = {
                                showOverflowMenu = false
                                onExportChatHistory(chat.id, "html")?.let { shareExportedFile(context, it, "text/html") }
                            },
                        )
                        DropdownMenuItem(
                            text = { Text("🗑 清空历史记录", color = palette.destructiveRed) },
                            onClick = {
                                showOverflowMenu = false
                                onClearChatHistory(chat.id)
                            },
                        )
                    }
                }
            }
        }
        }

        // In-Chat Search Bar (shared between Apple Messages and Classic headers)
        AnimatedVisibility(visible = state.inChatSearchActive) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.surfaceColor)
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TelegramIcon(TelegramGlyph.SEARCH, tint = palette.secondaryText, size = 18.dp)
                Spacer(Modifier.width(8.dp))
                BasicTextField(
                    value = state.inChatSearchQuery,
                    onValueChange = { onSetInChatSearch(true, it) },
                    singleLine = true,
                    textStyle = TextStyle(color = palette.primaryText, fontSize = 15.sp),
                    cursorBrush = SolidColor(palette.primaryAccent),
                    modifier = Modifier.weight(1f),
                    decorationBox = { inner ->
                        if (state.inChatSearchQuery.isEmpty()) {
                            Text("搜索当前对话消息…", color = palette.hintText, fontSize = 14.sp)
                        }
                        inner()
                    },
                )
                if (state.inChatSearchQuery.isNotBlank()) {
                    Text(
                        text = "${filteredMessages.size} 条匹配",
                        color = palette.primaryAccent,
                        fontSize = 12.sp,
                        modifier = Modifier.padding(end = 8.dp),
                    )
                }
                Box(
                    modifier = Modifier
                        .size(28.dp)
                        .clip(CircleShape)
                        .clickable { onSetInChatSearch(false, "") },
                    contentAlignment = Alignment.Center,
                ) {
                    TelegramIcon(TelegramGlyph.CLOSE, tint = palette.secondaryText, size = 16.dp)
                }
            }
        }

        // ====================================================================
        // 2. Pinned Message Banner, Floating Audio Player & Forum Topics Strip
        // ====================================================================
        if (pinnedMessages.isNotEmpty()) {
            val activePin = pinnedMessages[currentPinIdx]
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.surfaceColor)
                    .clickable {
                        val nextId = onCyclePinnedBanner(chat.id) ?: activePin.id
                        val idx = filteredMessages.indexOfFirst { it.id == nextId }
                        if (idx >= 0) {
                            coroutineScope.launch { listState.animateScrollToItem(idx) }
                        }
                    }
                    .padding(horizontal = 12.dp, vertical = 7.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    modifier = Modifier
                        .width(3.dp)
                        .height(32.dp)
                        .clip(RoundedCornerShape(2.dp))
                        .background(palette.primaryAccent),
                )
                Spacer(Modifier.width(10.dp))
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = if (pinnedMessages.size > 1) "📌 置顶消息 #${currentPinIdx + 1}/${pinnedMessages.size}" else "📌 置顶消息",
                        color = palette.primaryAccent,
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    Text(
                        text = activePin.text.lineSequence().firstOrNull().orEmpty(),
                        color = palette.secondaryText,
                        fontSize = 13.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                TelegramIcon(TelegramGlyph.PIN, tint = palette.secondaryText, size = 16.dp)
            }
            TelegramHairlineDivider()
        }

        // Floating Multi-Speed Audio / Voice Bar
        AnimatedVisibility(visible = state.audioPlayback.active) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.elevatedSurface)
                    .padding(horizontal = 12.dp, vertical = 6.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(
                        modifier = Modifier
                            .size(32.dp)
                            .clip(CircleShape)
                            .background(palette.primaryAccent)
                            .clickable {
                                val msg = allMessages.firstOrNull { it.id == state.audioPlayback.messageId }
                                if (msg != null) onPlayOrToggleAudio(msg, null) else onStopAudio()
                            },
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(
                            glyph = if (state.audioPlayback.isPlaying) TelegramGlyph.PAUSE else TelegramGlyph.PLAY,
                            tint = Color.White,
                            size = 16.dp,
                        )
                    }
                    Spacer(Modifier.width(10.dp))
                    Column(modifier = Modifier.weight(1f)) {
                        Text(
                            text = state.audioPlayback.title,
                            color = palette.primaryText,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.SemiBold,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                        val curSec = (state.audioPlayback.durationSeconds * state.audioPlayback.progressFraction).roundToInt()
                        Text(
                            text = "%02d:%02d / %02d:%02d".format(
                                curSec / 60,
                                curSec % 60,
                                state.audioPlayback.durationSeconds / 60,
                                state.audioPlayback.durationSeconds % 60,
                            ),
                            color = palette.secondaryText,
                            fontSize = 11.sp,
                        )
                    }
                    // Speed toggle 0.5x / 1x / 1.5x / 2x
                    Surface(
                        shape = RoundedCornerShape(10.dp),
                        color = palette.primaryAccent.copy(alpha = 0.16f),
                        modifier = Modifier.clickable(onClick = onCycleAudioSpeed),
                    ) {
                        Text(
                            text = "${state.audioPlayback.speedMultiplier}X",
                            color = palette.primaryAccent,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.padding(horizontal = 8.dp, vertical = 4.dp),
                        )
                    }
                    Spacer(Modifier.width(8.dp))
                    Box(
                        modifier = Modifier
                            .size(28.dp)
                            .clip(CircleShape)
                            .clickable(onClick = onStopAudio),
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.CLOSE, tint = palette.secondaryText, size = 16.dp)
                    }
                }
                Slider(
                    value = state.audioPlayback.progressFraction,
                    onValueChange = onSeekAudio,
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(22.dp),
                )
            }
        }

        // Forum Topics Strip (Supergroup Topics)
        if (chat.isForum) {
            LazyRow(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.surfaceColor)
                    .padding(vertical = 7.dp),
                contentPadding = PaddingValues(horizontal = 12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                item {
                    val allSelected = state.activeTopicId == null
                    Surface(
                        shape = RoundedCornerShape(16.dp),
                        color = if (allSelected) palette.primaryAccent else palette.elevatedSurface,
                        modifier = Modifier.clickable { onSelectForumTopic(null) },
                    ) {
                        Text(
                            text = "# 全部话题",
                            color = if (allSelected) Color.White else palette.primaryText,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                        )
                    }
                }
                items(topics, key = { it.id }) { topic ->
                    val selected = state.activeTopicId == topic.id
                    Surface(
                        shape = RoundedCornerShape(16.dp),
                        color = if (selected) Color(topic.accentColorHex) else palette.elevatedSurface,
                        modifier = Modifier.clickable { onSelectForumTopic(topic.id) },
                    ) {
                        Row(
                            modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(topic.iconEmoji, fontSize = 13.sp)
                            Spacer(Modifier.width(5.dp))
                            Text(
                                text = topic.title,
                                color = if (selected) Color.White else palette.primaryText,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.Medium,
                            )
                        }
                    }
                }
                item {
                    Surface(
                        shape = RoundedCornerShape(16.dp),
                        color = palette.primaryAccent.copy(alpha = 0.14f),
                        modifier = Modifier.clickable { showCreateTopicDialog = true },
                    ) {
                        Text(
                            text = "+ 新话题",
                            color = palette.primaryAccent,
                            fontSize = 12.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.padding(horizontal = 11.dp, vertical = 6.dp),
                        )
                    }
                }
            }
            TelegramHairlineDivider()
        }

        // ====================================================================
        // 3. Telegram Chat Wallpaper + Grouped Bubbles Stream
        // ====================================================================
        TelegramChatWallpaperBackground(
            modifier = Modifier.weight(1f),
        ) {
            if (filteredMessages.isEmpty()) {
                Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Surface(
                        shape = RoundedCornerShape(18.dp),
                        color = Color.Black.copy(alpha = 0.28f),
                    ) {
                        Column(
                            modifier = Modifier.padding(horizontal = 22.dp, vertical = 16.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            Text("✨ 暂无消息", color = Color.White, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                            Spacer(Modifier.height(4.dp))
                            Text("发送第一条消息，或输入 /mika 操控手机", color = Color.White.copy(alpha = 0.82f), fontSize = 12.sp)
                        }
                    }
                }
            } else {
                LazyColumn(
                    state = listState,
                    modifier = Modifier
                        .fillMaxSize()
                        .testTag("telegram-message-list"),
                    contentPadding = PaddingValues(horizontal = 8.dp, vertical = 10.dp),
                    verticalArrangement = Arrangement.spacedBy(3.dp),
                ) {
                    itemsIndexed(filteredMessages, key = { _, m -> m.id }) { idx, msg ->
                        val prev = filteredMessages.getOrNull(idx - 1)
                        val next = filteredMessages.getOrNull(idx + 1)
                        val groupedPrev = prev != null && prev.outgoing == msg.outgoing &&
                            prev.senderId == msg.senderId && prev.contentType != "service" && msg.contentType != "service"
                        val groupedNext = next != null && next.outgoing == msg.outgoing &&
                            next.senderId == msg.senderId && next.contentType != "service" && msg.contentType != "service"

                        if (msg.contentType == "service") {
                            Box(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(vertical = 6.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Surface(
                                    shape = RoundedCornerShape(14.dp),
                                    color = Color.Black.copy(alpha = 0.28f),
                                ) {
                                    Text(
                                        text = msg.text,
                                        color = Color.White,
                                        fontSize = 12.sp,
                                        modifier = Modifier.padding(horizontal = 12.dp, vertical = 5.dp),
                                    )
                                }
                            }
                        } else {
                            TelegramMessageBubbleRow(
                                chat = chat,
                                message = msg,
                                groupedPrev = groupedPrev,
                                groupedNext = groupedNext,
                                audioPlayback = state.audioPlayback,
                                onSwipeReply = { onSetReplyTarget(msg) },
                                onDoubleTapReact = { onToggleReaction(msg.id, "❤️") },
                                onLongPressMenu = { activeMenuMessage = msg },
                                onToggleReaction = { emoji -> onToggleReaction(msg.id, emoji) },
                                onVotePoll = { optId -> onVotePoll(msg.id, optId) },
                                onOpenMediaViewer = { onOpenMediaViewer(msg) },
                                onPlayOrToggleAudio = { frac -> onPlayOrToggleAudio(msg, frac) },
                                onOpenDiscussion = { onOpenDiscussionSheet(msg) },
                                onClickUrl = { url -> openExternalOrCustomTab(context, url) },
                                onClickSlashCommand = { cmd -> onSendMessage(cmd, false, null, null) },
                                onClickHashtagOrMention = { token -> onSetInChatSearch(true, token) },
                                onClickInlineButton = { btn ->
                                    onClickInlineButton(btn) { url -> openExternalOrCustomTab(context, url) }
                                },
                            )
                        }
                    }
                }
            }
        }

        // ====================================================================
        // 4. Full-Featured Telegram Composer
        // ====================================================================
        TelegramComposer(
            conversationId = chat.id,
            draftText = chat.draftText,
            replyTarget = state.replyTarget?.takeIf { it.conversationId == chat.id },
            editTarget = state.editTarget?.takeIf { it.conversationId == chat.id },
            recordingState = state.recordingState,
            slashCommands = state.slashCommands,
            stickerPacks = state.stickerPacks,
            gifCatalog = state.gifCatalog,
            onDraftChange = onDraftChange,
            onClearReplyOrEdit = {
                onSetReplyTarget(null)
                onSetEditTarget(null)
            },
            onSendMessage = onSendMessage,
            onToggleRecordMode = onToggleRecordMode,
            onStartRecording = onStartRecording,
            onDragRecording = onDragRecording,
            onCancelRecording = onCancelRecording,
            onFinishRecording = onFinishRecording,
            onSendSticker = onSendSticker,
            onSendGif = onSendGif,
            onSendPollOrQuiz = onSendPollOrQuiz,
            onSendLocation = onSendLocation,
            onSendContact = onSendContact,
            onSendMediaAttachment = onSendMediaAttachment,
            onOpenGlobalDharmaMiniApp = onOpenGlobalDharmaMiniApp,
            onPromptMikaTask = {
                mikaDialogInitialPrompt = ""
                showMikaTaskDialog = true
            },
        )
    }

    // ========================================================================
    // 5. Message Long-Press Floating Emoji Reaction Pill + Action Bottom Sheet
    // ========================================================================
    TelegramBottomSheet(
        visible = activeMenuMessage != null,
        onDismiss = { activeMenuMessage = null },
    ) {
        val msg = activeMenuMessage ?: return@TelegramBottomSheet
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            // Floating Emoji Reaction Pill Bar
            Surface(
                shape = RoundedCornerShape(28.dp),
                color = palette.elevatedSurface,
                tonalElevation = 4.dp,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .padding(horizontal = 10.dp, vertical = 8.dp),
                    horizontalArrangement = Arrangement.SpaceEvenly,
                ) {
                    QuickReactionEmojis.forEach { emoji ->
                        Text(
                            text = emoji,
                            fontSize = 24.sp,
                            modifier = Modifier
                                .clip(CircleShape)
                                .clickable {
                                    onToggleReaction(msg.id, emoji)
                                    activeMenuMessage = null
                                }
                                .padding(4.dp),
                        )
                    }
                }
            }

            Spacer(Modifier.height(4.dp))

            MessageActionSheetRow("↩️ 回复 (Reply)", palette.primaryText) {
                activeMenuMessage = null
                onSetReplyTarget(msg)
            }
            MessageActionSheetRow("📋 复制文本 (Copy)", palette.primaryText) {
                copyPlainText(context, msg.text)
                activeMenuMessage = null
            }
            MessageActionSheetRow("↪️ 转发到其他会话 (Forward)", palette.primaryText) {
                forwardCandidate = msg
                activeMenuMessage = null
            }
            if (msg.outgoing && msg.contentType == "text") {
                MessageActionSheetRow("✏️ 编辑消息 (Edit)", palette.primaryText) {
                    activeMenuMessage = null
                    onSetEditTarget(msg)
                }
            }
            MessageActionSheetRow(if (msg.pinned) "📌 取消置顶 (Unpin)" else "📌 置顶消息 (Pin)", palette.primaryText) {
                onTogglePinMessage(msg.id)
                activeMenuMessage = null
            }
            if (state.showTranslateButton && msg.text.isNotBlank()) {
                MessageActionSheetRow(
                    label = if (msg.translatedText != null) "🌐 隐藏即时译文" else "🌐 即时翻译消息 (Translate)",
                    tint = palette.primaryText,
                ) {
                    onTranslateMessage(msg.id)
                    activeMenuMessage = null
                }
            }
            MessageActionSheetRow("🤖 用 Mika 手机智能体执行此指令", palette.primaryAccent) {
                val prompt = msg.text.ifBlank { "执行聊天任务" }
                activeMenuMessage = null
                onRunWithMika(prompt)
            }
            TelegramHairlineDivider()
            MessageActionSheetRow("🗑 为我删除 (Delete for me)", palette.destructiveRed) {
                onDeleteMessage(msg.id, false)
                activeMenuMessage = null
            }
            MessageActionSheetRow("💥 为所有人双向撤回 (Revoke for everyone)", palette.destructiveRed) {
                onDeleteMessage(msg.id, true)
                activeMenuMessage = null
            }
        }
    }

    // Forward Target Picker Dialog
    if (forwardCandidate != null) {
        val src = forwardCandidate!!
        AlertDialog(
            onDismissRequest = { forwardCandidate = null },
            title = { Text("↪️ 选择转发目标会话") },
            text = {
                LazyColumn(modifier = Modifier.height(280.dp)) {
                    items(state.chats, key = { it.id }) { target ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable {
                                    onForwardMessage(src, target.id)
                                    forwardCandidate = null
                                }
                                .padding(vertical = 8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            TelegramAvatar(idSeed = target.id, title = target.title, size = 38.dp, kind = target.kind)
                            Spacer(Modifier.width(10.dp))
                            Column {
                                Text(target.title, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                Text(target.subtitle, color = palette.secondaryText, fontSize = 12.sp)
                            }
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { forwardCandidate = null }) { Text("取消") }
            },
        )
    }

    // Auto-Delete TTL Dialog
    if (showTtlDialog) {
        val options = listOf(
            0 to "关闭自动销毁",
            86_400 to "1 天后自动销毁",
            604_800 to "1 周后自动销毁",
            2_592_000 to "1 个月后自动销毁",
        )
        AlertDialog(
            onDismissRequest = { showTtlDialog = false },
            title = { Text("⏱ 消息自动销毁计时器 (TTL)") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    options.forEach { (sec, label) ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(10.dp))
                                .clickable {
                                    onSetChatTtl(chat.id, sec)
                                    showTtlDialog = false
                                }
                                .padding(10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(if (chat.ttlSeconds == sec) "🔘" else "⚪", fontSize = 16.sp)
                            Spacer(Modifier.width(10.dp))
                            Text(label, fontSize = 14.sp)
                        }
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = { showTtlDialog = false }) { Text("关闭") }
            },
        )
    }

    // Create Forum Topic Dialog
    if (showCreateTopicDialog) {
        var topicTitle by remember { mutableStateOf("") }
        var topicEmoji by remember { mutableStateOf("💡") }
        AlertDialog(
            onDismissRequest = { showCreateTopicDialog = false },
            title = { Text("💬 创建群组新话题 (Forum Topic)") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    OutlinedTextField(
                        value = topicEmoji,
                        onValueChange = { topicEmoji = it.take(4) },
                        label = { Text("话题图标 Emoji") },
                        singleLine = true,
                    )
                    OutlinedTextField(
                        value = topicTitle,
                        onValueChange = { topicTitle = it },
                        label = { Text("话题名称") },
                        singleLine = true,
                    )
                }
            },
            confirmButton = {
                Button(onClick = {
                    onCreateForumTopic(topicTitle, topicEmoji)
                    showCreateTopicDialog = false
                }) { Text("创建话题") }
            },
            dismissButton = {
                TextButton(onClick = { showCreateTopicDialog = false }) { Text("取消") }
            },
        )
    }

    // Mika Phone Agent Task Dispatch Dialog
    if (showMikaTaskDialog) {
        var taskInput by remember(mikaDialogInitialPrompt) { mutableStateOf(mikaDialogInitialPrompt) }
        AlertDialog(
            onDismissRequest = { showMikaTaskDialog = false },
            title = { Text("🤖 派发 Mika 本机操控任务") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        "Mika 将通过无障碍语义树 / 无线 ADB 虚拟屏幕执行您的操作，并在当前聊天实时回传步骤卡片：",
                        fontSize = 13.sp,
                        color = palette.secondaryText,
                    )
                    OutlinedTextField(
                        value = taskInput,
                        onValueChange = { taskInput = it },
                        placeholder = { Text("例如：打开系统设置开启蓝牙，并把结果发回群里") },
                        modifier = Modifier.fillMaxWidth(),
                    )
                }
            },
            confirmButton = {
                Button(onClick = {
                    onRunWithMika(taskInput.ifBlank { "检查手机通知并汇总" })
                    showMikaTaskDialog = false
                }) { Text("立即执行") }
            },
            dismissButton = {
                TextButton(onClick = { showMikaTaskDialog = false }) { Text("取消") }
            },
        )
    }

    // Channel Statistics Dialog
    state.channelStatsSnapshot?.let { stats ->
        AlertDialog(
            onDismissRequest = { onOpenChannelStats(null) },
            title = { Text("📊 频道数据统计 · ${chat.title}") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        StatMetricCard("总订阅者", "${stats.subscribers}", "+${stats.growth7dPercent}%")
                        StatMetricCard("通知开启率", "${stats.notificationsEnabledPercent}%", "高活跃")
                    }
                    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                        StatMetricCard("篇均阅读", "${stats.avgViewsPerPost}", "👁 曝光")
                        StatMetricCard("篇均转发/互动", "${stats.sharesPerPost} / ${stats.reactionsPerPost}", "🔥 传播")
                    }
                    Text("近 7 日阅读趋势", fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = palette.secondaryText)
                    Canvas(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(68.dp)
                            .clip(RoundedCornerShape(10.dp))
                            .background(palette.elevatedSurface)
                            .padding(8.dp),
                    ) {
                        val barW = size.width / (stats.dailyViewsBars.size * 1.8f)
                        stats.dailyViewsBars.forEachIndexed { i, v ->
                            val bh = (v / 100f).coerceIn(0.1f, 1f) * size.height
                            drawRoundRect(
                                color = Color(0xFF2AABEE),
                                topLeft = Offset(i * barW * 1.8f + barW * 0.3f, size.height - bh),
                                size = Size(barW, bh),
                                cornerRadius = CornerRadius(6f, 6f),
                            )
                        }
                    }
                }
            },
            confirmButton = {
                Button(onClick = { onOpenChannelStats(null) }) { Text("完成") }
            },
        )
    }

    // Channel Post Discussion Comments Bottom Sheet
    TelegramBottomSheet(
        visible = state.discussionSheetMessage != null,
        onDismiss = { onOpenDiscussionSheet(null) },
    ) {
        val rootMsg = state.discussionSheetMessage ?: return@TelegramBottomSheet
        val comments = state.discussionCommentsByMessage[rootMsg.id].orEmpty()
        var commentDraft by remember(rootMsg.id) { mutableStateOf("") }
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text("💬 频道帖子关联讨论 (${comments.size})", fontWeight = FontWeight.Bold, fontSize = 16.sp, color = palette.primaryText)
            Surface(shape = RoundedCornerShape(12.dp), color = palette.elevatedSurface, modifier = Modifier.fillMaxWidth()) {
                Text(
                    text = rootMsg.text,
                    color = palette.secondaryText,
                    fontSize = 13.sp,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(10.dp),
                )
            }
            LazyColumn(modifier = Modifier.height(200.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (comments.isEmpty()) {
                    item {
                        Text("抢先发表第一条评论…", color = palette.hintText, fontSize = 13.sp, modifier = Modifier.padding(vertical = 12.dp))
                    }
                }
                items(comments, key = { it.id }) { cmt ->
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(12.dp))
                            .background(palette.elevatedSurface)
                            .padding(10.dp),
                    ) {
                        Row(horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
                            Text(cmt.senderName, fontWeight = FontWeight.Bold, fontSize = 12.sp, color = palette.primaryAccent)
                            Text(cmt.time, fontSize = 11.sp, color = palette.hintText)
                        }
                        Spacer(Modifier.height(2.dp))
                        Text(cmt.text, fontSize = 14.sp, color = palette.primaryText)
                    }
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = commentDraft,
                    onValueChange = { commentDraft = it },
                    placeholder = { Text("发表评论…") },
                    singleLine = true,
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                Button(onClick = {
                    onPostDiscussionComment(rootMsg, commentDraft)
                    commentDraft = ""
                }) { Text("发送") }
            }
        }
    }
}

// ============================================================================
// Single Grouped Message Bubble + Swipe-to-Reply + Curved Tail + Entities
// ============================================================================

@OptIn(ExperimentalFoundationApi::class, ExperimentalLayoutApi::class)
@Composable
private fun TelegramMessageBubbleRow(
    chat: TelegramChatItem,
    message: TelegramMessageItem,
    groupedPrev: Boolean,
    groupedNext: Boolean,
    audioPlayback: FloatingAudioPlaybackState,
    onSwipeReply: () -> Unit,
    onDoubleTapReact: () -> Unit,
    onLongPressMenu: () -> Unit,
    onToggleReaction: (String) -> Unit,
    onVotePoll: (String) -> Unit,
    onOpenMediaViewer: () -> Unit,
    onPlayOrToggleAudio: (Float?) -> Unit,
    onOpenDiscussion: () -> Unit,
    onClickUrl: (String) -> Unit,
    onClickSlashCommand: (String) -> Unit,
    onClickHashtagOrMention: (String) -> Unit,
    onClickInlineButton: (InlineKeyboardButtonModel) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val haptic = LocalHapticFeedback.current
    var swipeOffsetPx by remember { mutableFloatStateOf(0f) }
    var replyTriggered by remember { mutableStateOf(false) }

    val baseRadius = palette.settings.boundedCornerDp
    val smallRadius = 5.dp
    val bubbleShape = if (message.outgoing) {
        RoundedCornerShape(
            topStart = baseRadius,
            topEnd = if (groupedPrev) smallRadius else baseRadius,
            bottomStart = baseRadius,
            bottomEnd = if (groupedNext) smallRadius else 4.dp,
        )
    } else {
        RoundedCornerShape(
            topStart = if (groupedPrev) smallRadius else baseRadius,
            topEnd = baseRadius,
            bottomStart = if (groupedNext) smallRadius else 4.dp,
            bottomEnd = baseRadius,
        )
    }

    val bubbleBg = if (message.outgoing) palette.outgoingBubble else palette.incomingBubble
    val textColor = if (message.outgoing) palette.outgoingText else palette.incomingText
    val metaColor = if (message.outgoing) palette.outgoingMeta else palette.incomingMeta

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .pointerInput(message.id) {
                detectHorizontalDragGestures(
                    onDragEnd = {
                        if (swipeOffsetPx <= -96f) {
                            onSwipeReply()
                        }
                        swipeOffsetPx = 0f
                        replyTriggered = false
                    },
                    onDragCancel = {
                        swipeOffsetPx = 0f
                        replyTriggered = false
                    },
                    onHorizontalDrag = { change, dragAmount ->
                        val next = (swipeOffsetPx + dragAmount).coerceIn(-160f, 0f)
                        if (next <= -96f && !replyTriggered) {
                            replyTriggered = true
                            haptic.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                        }
                        if (next != swipeOffsetPx) {
                            change.consume()
                            swipeOffsetPx = next
                        }
                    },
                )
            },
    ) {
        // Revealed Swipe-to-Reply Icon on Right
        if (swipeOffsetPx < -16f) {
            Box(
                modifier = Modifier
                    .align(Alignment.CenterEnd)
                    .padding(end = 12.dp)
                    .size(30.dp)
                    .clip(CircleShape)
                    .background(Color.Black.copy(alpha = 0.30f)),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.REPLY_ARROW, tint = Color.White, size = 16.dp)
            }
        }

        Column(
            modifier = Modifier
                .fillMaxWidth()
                .offset { IntOffset(swipeOffsetPx.roundToInt(), 0) },
            horizontalAlignment = if (message.outgoing) Alignment.End else Alignment.Start,
        ) {
            Row(
                verticalAlignment = Alignment.Bottom,
                horizontalArrangement = if (message.outgoing) Arrangement.End else Arrangement.Start,
            ) {
                // Group chat sender avatar on incoming bottom-of-group bubble
                if (!message.outgoing && chat.kind == ConversationKind.GROUP) {
                    if (!groupedNext) {
                        TelegramAvatar(idSeed = message.senderId, title = message.senderName, size = 30.dp)
                    } else {
                        Spacer(Modifier.width(30.dp))
                    }
                    Spacer(Modifier.width(6.dp))
                }

                // Incoming Curved Tail on bottom-left
                if (!message.outgoing && !groupedNext && message.contentType !in setOf("sticker", "videoNote")) {
                    Canvas(modifier = Modifier.size(width = 7.dp, height = 14.dp)) {
                        val p = Path().apply {
                            moveTo(size.width, 0f)
                            quadraticBezierTo(size.width * 0.4f, size.height * 0.85f, 0f, size.height)
                            lineTo(size.width, size.height)
                            close()
                        }
                        drawPath(p, color = bubbleBg)
                    }
                }

                // Frameless Sticker or Round Video Note vs Standard Bubble
                when (message.contentType) {
                    "sticker" -> {
                        Column(
                            horizontalAlignment = if (message.outgoing) Alignment.End else Alignment.Start,
                            modifier = Modifier
                                .combinedClickable(
                                    onClick = onDoubleTapReact,
                                    onLongClick = onLongPressMenu,
                                )
                                .padding(4.dp),
                        ) {
                            Text(text = message.stickerEmoji ?: "🪷", fontSize = 84.sp)
                            Surface(shape = RoundedCornerShape(10.dp), color = Color.Black.copy(alpha = 0.30f)) {
                                Text(
                                    text = "${message.stickerPackTitle ?: "贴纸"} · ${message.time}",
                                    color = Color.White,
                                    fontSize = 10.sp,
                                    modifier = Modifier.padding(horizontal = 7.dp, vertical = 2.dp),
                                )
                            }
                        }
                    }

                    "videoNote" -> {
                        RoundVideoNoteBubble(
                            message = message,
                            audioPlayback = audioPlayback,
                            onPlayToggle = { onPlayOrToggleAudio(null) },
                            onLongPress = onLongPressMenu,
                        )
                    }

                    else -> {
                        Surface(
                            shape = bubbleShape,
                            color = if (message.outgoing && palette.isAppleStyle) Color.Transparent else bubbleBg,
                            tonalElevation = if (palette.isAppleStyle) 0.dp else 1.dp,
                            shadowElevation = if (palette.isAppleStyle) 0.dp else 0.8.dp,
                            modifier = Modifier
                                .widthIn(min = 84.dp, max = 312.dp)
                                .then(
                                    if (message.outgoing && palette.isAppleStyle) {
                                        Modifier.background(palette.outgoingBubbleBrush, bubbleShape)
                                    } else {
                                        Modifier
                                    },
                                )
                                .combinedClickable(
                                    onClick = {},
                                    onDoubleClick = onDoubleTapReact,
                                    onLongClick = onLongPressMenu,
                                ),
                        ) {
                            Column(modifier = Modifier.padding(horizontal = 11.dp, vertical = 7.dp)) {
                                // Group Sender Title
                                if (!message.outgoing && !groupedPrev && (chat.kind == ConversationKind.GROUP || chat.kind == ConversationKind.CHANNEL)) {
                                    Text(
                                        text = message.senderName,
                                        color = palette.primaryAccent,
                                        fontSize = 12.sp,
                                        fontWeight = FontWeight.Bold,
                                    )
                                    Spacer(Modifier.height(2.dp))
                                }

                                // Forwarded Header
                                message.forwardHeader?.let { fwd ->
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        TelegramIcon(TelegramGlyph.FORWARD_ARROW, tint = palette.primaryAccent, size = 13.dp)
                                        Spacer(Modifier.width(4.dp))
                                        Text(
                                            text = "转发自 ${fwd.originName}",
                                            color = palette.primaryAccent,
                                            fontSize = 12.sp,
                                            fontWeight = FontWeight.SemiBold,
                                        )
                                    }
                                    Spacer(Modifier.height(3.dp))
                                }

                                // Reply Quote Header
                                message.replyHeader?.let { rep ->
                                    Row(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .clip(RoundedCornerShape(6.dp))
                                            .background(palette.primaryAccent.copy(alpha = 0.12f))
                                            .padding(horizontal = 7.dp, vertical = 4.dp),
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        Box(
                                            modifier = Modifier
                                                .width(2.5.dp)
                                                .height(26.dp)
                                                .background(palette.primaryAccent),
                                        )
                                        Spacer(Modifier.width(7.dp))
                                        Column {
                                            Text(rep.senderName, color = palette.primaryAccent, fontSize = 11.sp, fontWeight = FontWeight.Bold)
                                            Text(rep.quoteText, color = textColor.copy(alpha = 0.8f), fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                        }
                                    }
                                    Spacer(Modifier.height(5.dp))
                                }

                                // Specialized Content Bodies
                                when {
                                    message.poll != null -> {
                                        TelegramPollBubbleBody(
                                            poll = message.poll,
                                            textColor = textColor,
                                            accent = palette.primaryAccent,
                                            onVote = onVotePoll,
                                        )
                                    }

                                    message.contentType == "voice" || message.contentType == "audio" -> {
                                        TelegramVoiceWaveformBubbleBody(
                                            message = message,
                                            audioPlayback = audioPlayback,
                                            accent = if (message.outgoing) palette.outgoingMeta else palette.primaryAccent,
                                            textColor = textColor,
                                            onPlayOrScrub = onPlayOrToggleAudio,
                                        )
                                    }

                                    message.contentType == "gif" -> {
                                        Box(
                                            modifier = Modifier
                                                .fillMaxWidth()
                                                .height(132.dp)
                                                .clip(RoundedCornerShape(10.dp))
                                                .background(Brush.linearGradient(listOf(Color(0xFF1E3C72), Color(0xFF2A5298)))),
                                            contentAlignment = Alignment.Center,
                                        ) {
                                            Text(message.stickerEmoji ?: "✨", fontSize = 48.sp)
                                            Surface(
                                                shape = RoundedCornerShape(6.dp),
                                                color = Color.Black.copy(alpha = 0.5f),
                                                modifier = Modifier
                                                    .align(Alignment.TopStart)
                                                    .padding(8.dp),
                                            ) {
                                                Text("GIF", color = Color.White, fontSize = 10.sp, fontWeight = FontWeight.Bold, modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp))
                                            }
                                            Text(
                                                text = message.text,
                                                color = Color.White,
                                                fontSize = 12.sp,
                                                modifier = Modifier
                                                    .align(Alignment.BottomStart)
                                                    .padding(8.dp),
                                            )
                                        }
                                    }

                                    message.contentType in setOf("photo", "video", "document") -> {
                                        TelegramMediaAttachmentCard(
                                            message = message,
                                            accent = palette.primaryAccent,
                                            textColor = textColor,
                                            onOpen = onOpenMediaViewer,
                                        )
                                    }

                                    message.contentType == "location" -> {
                                        val context = LocalContext.current
                                        TelegramLocationCard(
                                            latitude = message.latitude ?: 31.2304,
                                            longitude = message.longitude ?: 121.4737,
                                            venueTitle = message.venueTitle ?: message.text,
                                            textColor = textColor,
                                            onOpenMap = {
                                                val lat = message.latitude ?: 31.2304
                                                val lon = message.longitude ?: 121.4737
                                                runCatching {
                                                    context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("geo:$lat,$lon?q=$lat,$lon")))
                                                }
                                            },
                                        )
                                    }

                                    message.contentType == "contact" -> {
                                        Row(
                                            modifier = Modifier
                                                .fillMaxWidth()
                                                .clip(RoundedCornerShape(10.dp))
                                                .background(palette.primaryAccent.copy(alpha = 0.10f))
                                                .padding(10.dp),
                                            verticalAlignment = Alignment.CenterVertically,
                                        ) {
                                            TelegramAvatar(idSeed = message.id, title = message.contactName ?: "C", size = 40.dp)
                                            Spacer(Modifier.width(10.dp))
                                            Column {
                                                Text(message.contactName ?: "联系人", fontWeight = FontWeight.Bold, color = textColor, fontSize = 14.sp)
                                                Text(message.contactPhone ?: "+86 138 0000 0000", color = palette.primaryAccent, fontSize = 12.sp)
                                            }
                                        }
                                    }

                                    message.contentType == "mikaTask" -> {
                                        MikaTaskLiveBubbleBody(message = message, textColor = textColor)
                                    }

                                    else -> {
                                        TelegramRichEntitiesText(
                                            message = message,
                                            textColor = textColor,
                                            linkColor = palette.linkColor,
                                            codeBg = palette.codeBackground,
                                            fontSizeSp = palette.settings.boundedFontSp,
                                            onClickUrl = onClickUrl,
                                            onClickSlashCommand = onClickSlashCommand,
                                            onClickHashtagOrMention = onClickHashtagOrMention,
                                            onDoubleTap = onDoubleTapReact,
                                            onLongPress = onLongPressMenu,
                                        )
                                    }
                                }

                                // Instant Translation Callout
                                if (message.translatedText != null) {
                                    Spacer(Modifier.height(6.dp))
                                    Surface(
                                        shape = RoundedCornerShape(8.dp),
                                        color = palette.primaryAccent.copy(alpha = 0.12f),
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

                                // Emoji Reaction Chips
                                if (message.reactions.isNotEmpty()) {
                                    Spacer(Modifier.height(6.dp))
                                    FlowRow(
                                        horizontalArrangement = Arrangement.spacedBy(5.dp),
                                        verticalArrangement = Arrangement.spacedBy(4.dp),
                                    ) {
                                        message.reactions.forEach { reaction ->
                                            Surface(
                                                shape = RoundedCornerShape(12.dp),
                                                color = if (reaction.chosenByMe) palette.primaryAccent.copy(alpha = 0.24f) else palette.codeBackground,
                                                modifier = Modifier.clickable { onToggleReaction(reaction.reaction) },
                                            ) {
                                                Text(
                                                    text = "${reaction.reaction} ${reaction.count}",
                                                    color = if (reaction.chosenByMe) palette.primaryAccent else textColor,
                                                    fontSize = 11.sp,
                                                    fontWeight = FontWeight.SemiBold,
                                                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                                                )
                                            }
                                        }
                                    }
                                }

                                // Timestamp, TTL, Views & Delivery Checkmarks Row
                                Spacer(Modifier.height(3.dp))
                                Row(
                                    modifier = Modifier.align(Alignment.End),
                                    verticalAlignment = Alignment.CenterVertically,
                                    horizontalArrangement = Arrangement.spacedBy(4.dp),
                                ) {
                                    if (message.pinned) {
                                        TelegramIcon(TelegramGlyph.PIN, tint = metaColor, size = 11.dp)
                                    }
                                    if (message.ttlSeconds != null && message.ttlSeconds > 0) {
                                        Text("⏱${formatTtlShort(message.ttlSeconds)}", color = metaColor, fontSize = 10.sp)
                                    }
                                    if (message.viewsCount != null) {
                                        Text("👁 ${message.viewsCount}", color = metaColor, fontSize = 11.sp)
                                    }
                                    if (message.edited) {
                                        Text("已编辑", color = metaColor, fontSize = 10.sp)
                                    }
                                    Text(text = message.time, color = metaColor, fontSize = 11.sp)
                                    if (message.outgoing) {
                                        val glyph = when (message.deliveryState) {
                                            "sending" -> TelegramGlyph.CLOCK_SENDING
                                            "sent" -> TelegramGlyph.CHECK_SINGLE
                                            else -> TelegramGlyph.CHECK_DOUBLE
                                        }
                                        TelegramIcon(glyph = glyph, tint = palette.checkMarkColor, size = 14.dp)
                                    }
                                }

                                // Channel Post Discussion Comments Entry
                                if (chat.kind == ConversationKind.CHANNEL || message.commentsCount != null) {
                                    Spacer(Modifier.height(5.dp))
                                    TelegramHairlineDivider()
                                    Row(
                                        modifier = Modifier
                                            .fillMaxWidth()
                                            .clickable(onClick = onOpenDiscussion)
                                            .padding(top = 5.dp),
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        Text(
                                            text = "💬 评论讨论 (${message.commentsCount ?: 0} 条)",
                                            color = palette.primaryAccent,
                                            fontSize = 12.sp,
                                            fontWeight = FontWeight.SemiBold,
                                        )
                                        Text("›", color = palette.primaryAccent, fontSize = 16.sp, fontWeight = FontWeight.Bold)
                                    }
                                }
                            }
                        }
                    }
                }

                // Outgoing Curved Tail on bottom-right
                if (message.outgoing && !groupedNext && message.contentType !in setOf("sticker", "videoNote")) {
                    Canvas(modifier = Modifier.size(width = 7.dp, height = 14.dp)) {
                        val p = Path().apply {
                            moveTo(0f, 0f)
                            quadraticBezierTo(size.width * 0.6f, size.height * 0.85f, size.width, size.height)
                            lineTo(0f, size.height)
                            close()
                        }
                        drawPath(p, color = bubbleBg)
                    }
                }
            }

            // Inline Bot Keyboard Rows (1..8 Columns) below bubble
            if (message.inlineKeyboard.isNotEmpty()) {
                Spacer(Modifier.height(4.dp))
                Column(
                    modifier = Modifier.widthIn(max = 312.dp),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    message.inlineKeyboard.forEach { row ->
                        Row(
                            modifier = Modifier.fillMaxWidth(),
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            row.boundedButtons.forEach { btn ->
                                Surface(
                                    shape = RoundedCornerShape(10.dp),
                                    color = Color.Black.copy(alpha = if (palette.isDark) 0.42f else 0.26f),
                                    modifier = Modifier
                                        .weight(1f)
                                        .clickable { onClickInlineButton(btn) },
                                ) {
                                    Row(
                                        modifier = Modifier.padding(horizontal = 8.dp, vertical = 8.dp),
                                        horizontalArrangement = Arrangement.Center,
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        Text(
                                            text = btn.text,
                                            color = Color.White,
                                            fontSize = 12.sp,
                                            fontWeight = FontWeight.SemiBold,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                        if (btn.kind == InlineKeyboardButtonKind.URL) {
                                            Spacer(Modifier.width(3.dp))
                                            Text("↗", color = Color.White.copy(alpha = 0.8f), fontSize = 10.sp)
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
// UTF-16 MessageEntity Renderer (Bold/Italic/Spoiler/Pre-Copy/URL/Commands)
// ============================================================================

@Composable
private fun TelegramRichEntitiesText(
    message: TelegramMessageItem,
    textColor: Color,
    linkColor: Color,
    codeBg: Color,
    fontSizeSp: androidx.compose.ui.unit.TextUnit,
    onClickUrl: (String) -> Unit,
    onClickSlashCommand: (String) -> Unit,
    onClickHashtagOrMention: (String) -> Unit,
    onDoubleTap: () -> Unit,
    onLongPress: () -> Unit,
) {
    val context = LocalContext.current
    val revealedSpoilers = remember(message.id) { mutableStateMapOf<Int, Boolean>() }
    var layoutResult by remember { mutableStateOf<TextLayoutResult?>(null) }

    val preEntities = remember(message.entities) {
        message.entities.filter { it.type == MessageEntityType.PRE }
    }

    val annotated: AnnotatedString = remember(message.text, message.entities, revealedSpoilers.toMap(), textColor, linkColor) {
        buildAnnotatedString {
            val raw = message.text
            // Mask unrevealed spoiler ranges with block shade glyphs of identical UTF-16 length
            val chars = raw.toCharArray()
            message.entities.forEachIndexed { idx, ent ->
                if (ent.type == MessageEntityType.SPOILER && revealedSpoilers[idx] != true) {
                    val start = ent.offset.coerceIn(0, chars.size)
                    val end = (ent.offset + ent.length).coerceIn(start, chars.size)
                    for (i in start until end) {
                        if (!chars[i].isWhitespace()) chars[i] = '▒'
                    }
                }
            }
            append(String(chars))

            message.entities.forEachIndexed { idx, ent ->
                val start = ent.offset.coerceIn(0, raw.length)
                val end = (ent.offset + ent.length).coerceIn(start, raw.length)
                if (start >= end) return@forEachIndexed
                when (ent.type) {
                    MessageEntityType.BOLD -> addStyle(SpanStyle(fontWeight = FontWeight.Bold), start, end)
                    MessageEntityType.ITALIC -> addStyle(SpanStyle(fontStyle = FontStyle.Italic), start, end)
                    MessageEntityType.UNDERLINE -> addStyle(SpanStyle(textDecoration = TextDecoration.Underline), start, end)
                    MessageEntityType.STRIKETHROUGH -> addStyle(SpanStyle(textDecoration = TextDecoration.LineThrough), start, end)
                    MessageEntityType.CODE, MessageEntityType.PRE -> addStyle(
                        SpanStyle(fontFamily = FontFamily.Monospace, background = codeBg),
                        start,
                        end,
                    )
                    MessageEntityType.SPOILER -> {
                        val isRevealed = revealedSpoilers[idx] == true
                        addStyle(
                            SpanStyle(
                                color = if (isRevealed) textColor else textColor.copy(alpha = 0.55f),
                                background = if (isRevealed) codeBg else textColor.copy(alpha = 0.18f),
                            ),
                            start,
                            end,
                        )
                        addStringAnnotation("SPOILER", idx.toString(), start, end)
                    }
                    MessageEntityType.URL -> {
                        val url = TelegramEntitiesEngine.substringSafeUtf16(raw, start, end - start)
                        addStyle(SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline), start, end)
                        addStringAnnotation("URL", url, start, end)
                    }
                    MessageEntityType.TEXT_URL -> {
                        addStyle(SpanStyle(color = linkColor, textDecoration = TextDecoration.Underline), start, end)
                        addStringAnnotation("URL", ent.url ?: "", start, end)
                    }
                    MessageEntityType.BOT_COMMAND -> {
                        val cmd = TelegramEntitiesEngine.substringSafeUtf16(raw, start, end - start)
                        addStyle(SpanStyle(color = linkColor, fontWeight = FontWeight.Medium), start, end)
                        addStringAnnotation("CMD", cmd, start, end)
                    }
                    MessageEntityType.MENTION, MessageEntityType.HASHTAG -> {
                        val tok = TelegramEntitiesEngine.substringSafeUtf16(raw, start, end - start)
                        addStyle(SpanStyle(color = linkColor), start, end)
                        addStringAnnotation("TAG", tok, start, end)
                    }
                }
            }
        }
    }

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            text = annotated,
            color = textColor,
            fontSize = fontSizeSp,
            lineHeight = (fontSizeSp.value * 1.35f).sp,
            onTextLayout = { layoutResult = it },
            modifier = Modifier.pointerInput(annotated) {
                detectTapGestures(
                    onDoubleTap = { onDoubleTap() },
                    onLongPress = { onLongPress() },
                    onTap = { pos ->
                        val layout = layoutResult ?: return@detectTapGestures
                        val offset = layout.getOffsetForPosition(pos)
                        annotated.getStringAnnotations("SPOILER", offset, offset).firstOrNull()?.let { ann ->
                            ann.item.toIntOrNull()?.let { idx -> revealedSpoilers[idx] = !(revealedSpoilers[idx] ?: false) }
                            return@detectTapGestures
                        }
                        annotated.getStringAnnotations("URL", offset, offset).firstOrNull()?.let { ann ->
                            if (ann.item.isNotBlank()) onClickUrl(ann.item)
                            return@detectTapGestures
                        }
                        annotated.getStringAnnotations("CMD", offset, offset).firstOrNull()?.let { ann ->
                            onClickSlashCommand(ann.item)
                            return@detectTapGestures
                        }
                        annotated.getStringAnnotations("TAG", offset, offset).firstOrNull()?.let { ann ->
                            onClickHashtagOrMention(ann.item)
                        }
                    },
                )
            },
        )

        // Dedicated Copy-Code Cards for PRE code blocks
        preEntities.forEach { pre ->
            val codeSnippet = TelegramEntitiesEngine.substringSafeUtf16(message.text, pre.offset, pre.length)
            Surface(
                shape = RoundedCornerShape(8.dp),
                color = codeBg,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Column(modifier = Modifier.padding(8.dp)) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = pre.language?.uppercase() ?: "CODE",
                            fontSize = 10.sp,
                            fontWeight = FontWeight.Bold,
                            color = linkColor,
                        )
                        Row(
                            modifier = Modifier
                                .clip(RoundedCornerShape(6.dp))
                                .clickable { copyPlainText(context, codeSnippet) }
                                .padding(horizontal = 6.dp, vertical = 2.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            TelegramIcon(TelegramGlyph.COPY, tint = linkColor, size = 12.dp)
                            Spacer(Modifier.width(4.dp))
                            Text("复制代码", fontSize = 11.sp, color = linkColor, fontWeight = FontWeight.SemiBold)
                        }
                    }
                    Spacer(Modifier.height(4.dp))
                    Text(
                        text = codeSnippet,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 12.sp,
                        color = textColor,
                    )
                }
            }
        }
    }
}

// ============================================================================
// Specialized Bubble Bodies: 100-Bar Voice, Round Video Note, Poll/Quiz, Media
// ============================================================================

@Composable
private fun TelegramVoiceWaveformBubbleBody(
    message: TelegramMessageItem,
    audioPlayback: FloatingAudioPlaybackState,
    accent: Color,
    textColor: Color,
    onPlayOrScrub: (Float?) -> Unit,
) {
    val isThisPlaying = audioPlayback.active && audioPlayback.messageId == message.id && audioPlayback.isPlaying
    val progress = if (audioPlayback.active && audioPlayback.messageId == message.id) audioPlayback.progressFraction else 0f
    val bars = remember(message.waveformBars, message.id) {
        TelegramWaveformCodec.normalizeTo100Bars(message.waveformBars, seedHint = message.id)
    }

    Row(
        modifier = Modifier.width(236.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(42.dp)
                .clip(CircleShape)
                .background(accent)
                .clickable { onPlayOrScrub(null) },
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(
                glyph = if (isThisPlaying) TelegramGlyph.PAUSE else TelegramGlyph.PLAY,
                tint = Color.White,
                size = 20.dp,
            )
        }
        Spacer(Modifier.width(10.dp))
        Column(modifier = Modifier.weight(1f)) {
            // 100-Bar 5-Bit Scrubbable Waveform Canvas
            Canvas(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(28.dp)
                    .pointerInput(message.id) {
                        detectTapGestures { offset ->
                            val frac = (offset.x / size.width.coerceAtLeast(1)).coerceIn(0f, 0.99f)
                            onPlayOrScrub(frac)
                        }
                    },
            ) {
                val totalBars = 50 // render 50 crisp paired columns from 100-bar sample
                val stepX = size.width / totalBars
                val barWidth = (stepX * 0.58f).coerceAtLeast(2f)
                for (i in 0 until totalBars) {
                    val sample = bars[(i * 2).coerceAtMost(bars.lastIndex)]
                    val norm = (sample.toFloat() / TelegramWaveformCodec.MAX_BAR_VALUE).coerceIn(0.12f, 1f)
                    val bh = norm * size.height
                    val played = (i.toFloat() / totalBars) <= progress
                    drawRoundRect(
                        color = if (played) accent else accent.copy(alpha = 0.34f),
                        topLeft = Offset(i * stepX, (size.height - bh) / 2f),
                        size = Size(barWidth, bh),
                        cornerRadius = CornerRadius(barWidth / 2f, barWidth / 2f),
                    )
                }
            }
            Spacer(Modifier.height(3.dp))
            val dur = message.durationSeconds.coerceAtLeast(1)
            Text(
                text = "%02d:%02d · 5-bit 100-bar 波形".format(dur / 60, dur % 60),
                color = textColor.copy(alpha = 0.72f),
                fontSize = 11.sp,
            )
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun RoundVideoNoteBubble(
    message: TelegramMessageItem,
    audioPlayback: FloatingAudioPlaybackState,
    onPlayToggle: () -> Unit,
    onLongPress: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val playing = audioPlayback.active && audioPlayback.messageId == message.id && audioPlayback.isPlaying
    val progress = if (audioPlayback.active && audioPlayback.messageId == message.id) audioPlayback.progressFraction else 0.25f

    Box(
        modifier = Modifier
            .size(192.dp)
            .combinedClickable(onClick = onPlayToggle, onLongClick = onLongPress),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier
                .size(182.dp)
                .clip(CircleShape)
                .background(
                    Brush.radialGradient(
                        listOf(Color(0xFF284B63), Color(0xFF152636), Color(0xFF0B141E)),
                    ),
                ),
            contentAlignment = Alignment.Center,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                TelegramIcon(
                    glyph = if (playing) TelegramGlyph.PAUSE else TelegramGlyph.VIDEO_ROUND,
                    tint = Color.White,
                    size = 38.dp,
                )
                Spacer(Modifier.height(6.dp))
                Text(
                    text = if (playing) "正在播放圆视频…" else "点击播放圆视频",
                    color = Color.White.copy(alpha = 0.88f),
                    fontSize = 12.sp,
                )
            }
            Surface(
                shape = RoundedCornerShape(10.dp),
                color = Color.Black.copy(alpha = 0.48f),
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .padding(bottom = 14.dp),
            ) {
                Text(
                    text = "%02d:%02d · ${message.time}".format(message.durationSeconds / 60, message.durationSeconds % 60),
                    color = Color.White,
                    fontSize = 10.sp,
                    modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp),
                )
            }
        }
        // Circular Progress Ring around Round Video Note
        Canvas(modifier = Modifier.fillMaxSize()) {
            drawArc(
                color = palette.primaryAccent,
                startAngle = -90f,
                sweepAngle = 360f * progress.coerceIn(0.06f, 1f),
                useCenter = false,
                style = Stroke(width = 3.dp.toPx()),
            )
        }
    }
}

@Composable
private fun TelegramPollBubbleBody(
    poll: TelegramPollModel,
    textColor: Color,
    accent: Color,
    onVote: (String) -> Unit,
) {
    Column(modifier = Modifier.width(264.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
        Text(text = poll.question, color = textColor, fontWeight = FontWeight.Bold, fontSize = 15.sp)
        Text(
            text = buildString {
                append(if (poll.isQuiz) "🎯 知识竞答 (Quiz)" else "📊 群组投票")
                append(if (poll.isAnonymous) " · 匿名" else " · 实名")
                if (poll.multipleAnswers) append(" · 多选")
            },
            color = textColor.copy(alpha = 0.65f),
            fontSize = 11.sp,
        )

        val total = poll.totalVotes.coerceAtLeast(1)
        poll.options.forEach { opt ->
            val pct = if (poll.hasVoted) ((opt.voterCount * 100f) / total).roundToInt() else 0
            val optionTint = when {
                poll.isQuiz && poll.hasVoted && opt.isCorrect -> Color(0xFF2FA84F)
                poll.isQuiz && poll.hasVoted && opt.chosen && !opt.isCorrect -> Color(0xFFE53935)
                else -> accent
            }
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(10.dp))
                    .background(optionTint.copy(alpha = 0.10f))
                    .clickable { onVote(opt.id) }
                    .padding(horizontal = 10.dp, vertical = 8.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.SpaceBetween,
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        text = buildString {
                            if (opt.chosen) append(if (poll.isQuiz && !opt.isCorrect) "✗ " else "✓ ")
                            else if (poll.isQuiz && poll.hasVoted && opt.isCorrect) append("✓ ")
                            append(opt.text)
                        },
                        color = textColor,
                        fontSize = 13.sp,
                        fontWeight = if (opt.chosen) FontWeight.Bold else FontWeight.Normal,
                        modifier = Modifier.weight(1f),
                    )
                    if (poll.hasVoted) {
                        Text("$pct%", color = optionTint, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                    }
                }
                if (poll.hasVoted) {
                    Spacer(Modifier.height(4.dp))
                    LinearProgressIndicator(
                        progress = { (pct / 100f).coerceIn(0f, 1f) },
                        color = optionTint,
                        trackColor = optionTint.copy(alpha = 0.18f),
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(5.dp)
                            .clip(RoundedCornerShape(3.dp)),
                    )
                }
            }
        }

        if (poll.isQuiz && poll.hasVoted && !poll.explanation.isNullOrBlank()) {
            Surface(shape = RoundedCornerShape(8.dp), color = accent.copy(alpha = 0.14f)) {
                Text(
                    text = "💡 解析：${poll.explanation}",
                    color = textColor,
                    fontSize = 12.sp,
                    modifier = Modifier.padding(8.dp),
                )
            }
        }

        Text(
            text = "共 ${poll.options.sumOf { it.voterCount }} 人参与",
            color = textColor.copy(alpha = 0.6f),
            fontSize = 11.sp,
        )
    }
}

@Composable
private fun TelegramMediaAttachmentCard(
    message: TelegramMessageItem,
    accent: Color,
    textColor: Color,
    onOpen: () -> Unit,
) {
    Column(
        modifier = Modifier
            .width(240.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(accent.copy(alpha = 0.10f))
            .clickable(onClick = onOpen)
            .padding(10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (message.contentType in setOf("photo", "video")) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(128.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(Brush.linearGradient(listOf(accent.copy(alpha = 0.45f), Color(0xFF1F3A52)))),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(
                    glyph = if (message.contentType == "video") TelegramGlyph.PLAY else TelegramGlyph.CAMERA,
                    tint = Color.White,
                    size = 34.dp,
                )
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier = Modifier
                    .size(36.dp)
                    .clip(CircleShape)
                    .background(accent),
                contentAlignment = Alignment.Center,
            ) {
                TelegramIcon(TelegramGlyph.PAPERCLIP, tint = Color.White, size = 18.dp)
            }
            Spacer(Modifier.width(9.dp))
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = message.mediaFileName ?: message.text,
                    color = textColor,
                    fontWeight = FontWeight.SemiBold,
                    fontSize = 13.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                val kb = (message.mediaSizeBytes / 1024).coerceAtLeast(1)
                Text(
                    text = "${message.contentType.uppercase()} · ${kb} KB · 点击查看",
                    color = textColor.copy(alpha = 0.65f),
                    fontSize = 11.sp,
                )
            }
        }
    }
}

@Composable
private fun TelegramLocationCard(
    latitude: Double,
    longitude: Double,
    venueTitle: String,
    textColor: Color,
    onOpenMap: () -> Unit,
) {
    Column(
        modifier = Modifier
            .width(240.dp)
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onOpenMap),
    ) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(110.dp)
                .background(Brush.linearGradient(listOf(Color(0xFFC8E6C9), Color(0xFFBBDEFB)))),
            contentAlignment = Alignment.Center,
        ) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val step = size.width / 6f
                for (i in 1..5) {
                    drawLine(Color.White.copy(alpha = 0.6f), Offset(i * step, 0f), Offset(i * step, size.height), 2f)
                    drawLine(Color.White.copy(alpha = 0.6f), Offset(0f, i * (size.height / 5f)), Offset(size.width, i * (size.height / 5f)), 2f)
                }
            }
            Text("📍", fontSize = 32.sp)
        }
        Column(modifier = Modifier.padding(top = 6.dp)) {
            Text(venueTitle, color = textColor, fontWeight = FontWeight.Bold, fontSize = 13.sp)
            Text("%.4f, %.4f · 点击导航".format(latitude, longitude), color = textColor.copy(alpha = 0.68f), fontSize = 11.sp)
        }
    }
}

@Composable
private fun MikaTaskLiveBubbleBody(
    message: TelegramMessageItem,
    textColor: Color,
) {
    Column(modifier = Modifier.width(268.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.SpaceBetween, modifier = Modifier.fillMaxWidth()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                TelegramIcon(TelegramGlyph.MIKA_PHONE, tint = Color(0xFF00BFA5), size = 18.dp)
                Spacer(Modifier.width(6.dp))
                Text("Mika 本机执行任务", color = Color(0xFF00BFA5), fontWeight = FontWeight.Bold, fontSize = 13.sp)
            }
            Surface(shape = RoundedCornerShape(8.dp), color = Color(0xFF00BFA5).copy(alpha = 0.16f)) {
                Text(
                    text = message.mikaTaskStatus?.uppercase() ?: "RUNNING",
                    color = Color(0xFF00BFA5),
                    fontSize = 10.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
                )
            }
        }
        Text(message.text, color = textColor, fontSize = 13.sp)
        if (message.mikaStepLogs.isNotEmpty()) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color.Black.copy(alpha = 0.08f))
                    .padding(8.dp),
                verticalArrangement = Arrangement.spacedBy(3.dp),
            ) {
                message.mikaStepLogs.forEach { step ->
                    Text("• $step", color = textColor.copy(alpha = 0.85f), fontSize = 11.sp, fontFamily = FontFamily.Monospace)
                }
            }
        }
    }
}

@Composable
private fun MessageActionSheetRow(label: String, tint: Color, onClick: () -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text = label, color = tint, fontSize = 15.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun StatMetricCard(title: String, value: String, delta: String) {
    val palette = LocalTelegramPalette.current
    Surface(
        shape = RoundedCornerShape(12.dp),
        color = palette.elevatedSurface,
        modifier = Modifier.width(126.dp),
    ) {
        Column(modifier = Modifier.padding(10.dp)) {
            Text(value, fontWeight = FontWeight.Bold, fontSize = 16.sp, color = palette.primaryText)
            Text(title, fontSize = 11.sp, color = palette.secondaryText)
            Text(delta, fontSize = 11.sp, color = Color(0xFF2FA84F), fontWeight = FontWeight.SemiBold)
        }
    }
}

private fun formatTtlShort(ttlSeconds: Int): String = when {
    ttlSeconds <= 0 -> "关闭"
    ttlSeconds <= 86_400 -> "1天"
    ttlSeconds <= 604_800 -> "1周"
    else -> "1月"
}

private fun copyPlainText(context: Context, text: String) {
    val cm = context.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
    cm.setPrimaryClip(ClipData.newPlainText("TelegramMessage", text))
}

private fun openExternalOrCustomTab(context: Context, rawUrl: String) {
    val normalized = if (rawUrl.startsWith("http://") || rawUrl.startsWith("https://")) rawUrl else "https://$rawUrl"
    val uri = runCatching { Uri.parse(normalized) }.getOrNull() ?: return
    runCatching {
        CustomTabsIntent.Builder().build().launchUrl(context, uri)
    }.onFailure {
        runCatching { context.startActivity(Intent(Intent.ACTION_VIEW, uri).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
    }
}

private fun shareExportedFile(context: Context, file: File, mimeType: String) {
    runCatching {
        val uri = FileProvider.getUriForFile(context, "${context.packageName}.media", file)
        val send = Intent(Intent.ACTION_SEND).apply {
            type = mimeType
            putExtra(Intent.EXTRA_STREAM, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        context.startActivity(Intent.createChooser(send, "分享导出的 Telegram 聊天记录"))
    }
}

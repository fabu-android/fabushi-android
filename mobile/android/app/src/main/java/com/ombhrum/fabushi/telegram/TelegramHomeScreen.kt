package com.ombhrum.fabushi.telegram

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SmallFloatingActionButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.ConversationKind
import kotlin.math.roundToInt
import kotlinx.coroutines.delay

@Composable
fun TelegramHomeScreen(
    state: TelegramUiState,
    onOpenDrawer: () -> Unit,
    onLockAppNow: () -> Unit,
    onSetGlobalSearch: (Boolean, String, GlobalSearchFilter) -> Unit,
    onSelectFolder: (String) -> Unit,
    onOpenFolderSettings: () -> Unit,
    onToggleArchiveExpanded: () -> Unit,
    onOpenStory: (String) -> Unit,
    onCloseStory: () -> Unit,
    onCreateStory: (String, String, Int) -> Unit,
    onOpenChat: (String) -> Unit,
    onToggleChatSelection: (String) -> Unit,
    onClearChatSelection: () -> Unit,
    onPinChat: (String) -> Unit,
    onArchiveChat: (String) -> Unit,
    onMuteChat: (String) -> Unit,
    onToggleUnreadChat: (String) -> Unit,
    onRequestCreateKind: (ConversationKind) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    var showCreateStoryDialog by remember { mutableStateOf(false) }
    var showNewMessageSheet by remember { mutableStateOf(false) }
    var storiesCollapsed by remember { mutableStateOf(false) }

    val activeFolder = state.folderTabs.firstOrNull { it.id == state.selectedFolderId } ?: state.folderTabs.first()
    val archivedChats = remember(state.chats) { state.chats.filter { it.isArchived } }
    val visibleChats = remember(state.chats, activeFolder, state.globalSearchActive, state.globalSearchQuery) {
        if (state.globalSearchActive && state.globalSearchQuery.isNotBlank()) {
            val q = state.globalSearchQuery.trim()
            state.chats.filter {
                it.title.contains(q, ignoreCase = true) ||
                    it.preview.contains(q, ignoreCase = true) ||
                    (it.username?.contains(q, ignoreCase = true) == true)
            }
        } else {
            state.chats.filter { TelegramFolderFilter.matches(activeFolder, it) }
        }
    }

    val globalMessageMatches = remember(state.messagesByChat, state.globalSearchActive, state.globalSearchQuery, state.globalSearchFilter) {
        if (!state.globalSearchActive) emptyList()
        else {
            val q = state.globalSearchQuery.trim()
            state.messagesByChat.values.flatten().filter { msg ->
                val filterMatch = when (state.globalSearchFilter) {
                    GlobalSearchFilter.CHATS -> q.isNotBlank() && msg.text.contains(q, ignoreCase = true)
                    GlobalSearchFilter.MEDIA -> msg.contentType in setOf("photo", "video", "videoNote", "gif")
                    GlobalSearchFilter.LINKS -> msg.entities.any { it.type == MessageEntityType.URL || it.type == MessageEntityType.TEXT_URL } || msg.text.contains("http")
                    GlobalSearchFilter.FILES -> msg.contentType == "document"
                    GlobalSearchFilter.MUSIC -> msg.contentType == "audio"
                    GlobalSearchFilter.VOICE -> msg.contentType == "voice"
                }
                filterMatch && (q.isBlank() || msg.text.contains(q, ignoreCase = true) || (msg.mediaFileName?.contains(q, ignoreCase = true) == true))
            }
        }
    }

    Box(modifier = Modifier.fillMaxSize().background(palette.surfaceColor)) {
        Column(modifier = Modifier.fillMaxSize()) {
            when {
                state.selectedChatIds.isNotEmpty() -> {
                    BackHandler(onBack = onClearChatSelection)
                    val firstSelected = state.selectedChatIds.first()
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(palette.topBarBackground)
                            .statusBarsPadding()
                            .height(56.dp)
                            .padding(horizontal = 12.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Box(
                            modifier = Modifier.size(40.dp).clip(CircleShape).clickable(onClick = onClearChatSelection),
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.CLOSE, tint = palette.topBarContent)
                        }
                        Spacer(Modifier.width(12.dp))
                        Text(
                            text = "已选 ${state.selectedChatIds.size} 项",
                            color = palette.topBarContent,
                            fontSize = 18.sp,
                            fontWeight = FontWeight.SemiBold,
                            modifier = Modifier.weight(1f),
                        )
                        IconButtonCircle(TelegramGlyph.PIN, palette.topBarContent) {
                            state.selectedChatIds.forEach(onPinChat)
                        }
                        IconButtonCircle(TelegramGlyph.MUTE, palette.topBarContent) {
                            state.selectedChatIds.forEach(onMuteChat)
                        }
                        IconButtonCircle(TelegramGlyph.ARCHIVE, palette.topBarContent) {
                            state.selectedChatIds.forEach(onArchiveChat)
                        }
                        TextButton(onClick = { onToggleUnreadChat(firstSelected) }) {
                            Text("未读/已读", color = palette.topBarContent, fontSize = 12.sp)
                        }
                    }
                }

                state.globalSearchActive -> {
                    BackHandler { onSetGlobalSearch(false, "", GlobalSearchFilter.CHATS) }
                    Column(
                        modifier = Modifier
                            .fillMaxWidth()
                            .background(palette.topBarBackground)
                            .statusBarsPadding(),
                    ) {
                        Row(
                            modifier = Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            IconButtonCircle(TelegramGlyph.BACK, palette.topBarContent) {
                                onSetGlobalSearch(false, "", GlobalSearchFilter.CHATS)
                            }
                            Spacer(Modifier.width(8.dp))
                            BasicTextField(
                                value = state.globalSearchQuery,
                                onValueChange = { onSetGlobalSearch(true, it, state.globalSearchFilter) },
                                singleLine = true,
                                textStyle = TextStyle(color = palette.topBarContent, fontSize = 16.sp),
                                cursorBrush = SolidColor(Color.White),
                                modifier = Modifier
                                    .weight(1f)
                                    .testTag("telegram-global-search-input"),
                                decorationBox = { inner ->
                                    if (state.globalSearchQuery.isEmpty()) {
                                        Text("搜索对话、消息、链接、语音…", color = palette.topBarSubtitle, fontSize = 15.sp)
                                    }
                                    inner()
                                },
                            )
                            if (state.globalSearchQuery.isNotEmpty()) {
                                IconButtonCircle(TelegramGlyph.CLOSE, palette.topBarContent) {
                                    onSetGlobalSearch(true, "", state.globalSearchFilter)
                                }
                            }
                        }
                        LazyRow(
                            contentPadding = PaddingValues(horizontal = 12.dp, vertical = 6.dp),
                            horizontalArrangement = Arrangement.spacedBy(8.dp),
                        ) {
                            items(GlobalSearchFilter.entries) { filter ->
                                val selected = state.globalSearchFilter == filter
                                Box(
                                    modifier = Modifier
                                        .clip(RoundedCornerShape(14.dp))
                                        .background(if (selected) Color.White.copy(alpha = 0.22f) else Color.Transparent)
                                        .clickable { onSetGlobalSearch(true, state.globalSearchQuery, filter) }
                                        .padding(horizontal = 12.dp, vertical = 6.dp),
                                ) {
                                    Text(
                                        text = filter.label,
                                        color = if (selected) Color.White else palette.topBarSubtitle,
                                        fontSize = 13.sp,
                                        fontWeight = if (selected) FontWeight.Bold else FontWeight.Medium,
                                    )
                                }
                            }
                        }
                    }
                }

                else -> {
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
                                .padding(horizontal = 8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Box(
                                modifier = Modifier
                                    .size(44.dp)
                                    .clip(CircleShape)
                                    .clickable(onClick = onOpenDrawer)
                                    .testTag("telegram-drawer-button"),
                                contentAlignment = Alignment.Center,
                            ) {
                                TelegramIcon(TelegramGlyph.HAMBURGER, tint = palette.topBarContent, size = 22.dp)
                            }
                            Spacer(Modifier.width(8.dp))
                            Column(
                                modifier = Modifier
                                    .weight(1f)
                                    .clickable { storiesCollapsed = !storiesCollapsed },
                            ) {
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Text(
                                        text = state.syncStatusLabel,
                                        color = palette.topBarContent,
                                        fontSize = 19.sp,
                                        fontWeight = FontWeight.Bold,
                                    )
                                    val totalUnread = state.chats.filter { !it.isArchived }.sumOf { it.unreadCount }
                                    if (totalUnread > 0) {
                                        Spacer(Modifier.width(8.dp))
                                        Box(
                                            modifier = Modifier
                                                .clip(RoundedCornerShape(10.dp))
                                                .background(Color.White.copy(alpha = 0.22f))
                                                .padding(horizontal = 7.dp, vertical = 1.dp),
                                        ) {
                                            Text(totalUnread.toString(), color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                                        }
                                    }
                                }
                            }
                            IconButtonCircle(
                                glyph = if (state.passcodeConfigured) TelegramGlyph.LOCK_OPEN else TelegramGlyph.LOCK_CLOSED,
                                tint = palette.topBarContent,
                                testTag = "telegram-passcode-lock-button",
                                onClick = onLockAppNow,
                            )
                            IconButtonCircle(
                                glyph = TelegramGlyph.SEARCH,
                                tint = palette.topBarContent,
                                testTag = "telegram-search-button",
                                onClick = { onSetGlobalSearch(true, "", GlobalSearchFilter.CHATS) },
                            )
                        }

                        AnimatedVisibility(visible = !storiesCollapsed && state.stories.isNotEmpty()) {
                            LazyRow(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(bottom = 10.dp),
                                contentPadding = PaddingValues(horizontal = 14.dp),
                                horizontalArrangement = Arrangement.spacedBy(14.dp),
                            ) {
                                item(key = "add-story") {
                                    Column(
                                        horizontalAlignment = Alignment.CenterHorizontally,
                                        modifier = Modifier
                                            .width(62.dp)
                                            .clickable { showCreateStoryDialog = true },
                                    ) {
                                        Box(modifier = Modifier.size(54.dp), contentAlignment = Alignment.Center) {
                                            TelegramAvatar(
                                                idSeed = state.activeAccount?.avatarSeed ?: "me",
                                                title = state.activeAccount?.displayName ?: "我",
                                                size = 50.dp,
                                            )
                                            Box(
                                                modifier = Modifier
                                                    .align(Alignment.BottomEnd)
                                                    .size(20.dp)
                                                    .clip(CircleShape)
                                                    .background(Color.White),
                                                contentAlignment = Alignment.Center,
                                            ) {
                                                TelegramIcon(TelegramGlyph.PLUS, tint = palette.topBarBackground, size = 13.dp)
                                            }
                                        }
                                        Spacer(Modifier.height(4.dp))
                                        Text("发快拍", color = palette.topBarContent, fontSize = 11.sp, maxLines = 1)
                                    }
                                }
                                items(state.stories, key = { it.id }) { story ->
                                    Column(
                                        horizontalAlignment = Alignment.CenterHorizontally,
                                        modifier = Modifier
                                            .width(62.dp)
                                            .clickable { onOpenStory(story.id) },
                                    ) {
                                        val ringBrush = if (story.viewed) {
                                            Brush.linearGradient(listOf(Color.White.copy(alpha = 0.35f), Color.White.copy(alpha = 0.35f)))
                                        } else {
                                            Brush.linearGradient(listOf(Color(0xFF38EF7D), Color(0xFF2AABEE), Color(0xFFFFD54F)))
                                        }
                                        Box(
                                            modifier = Modifier
                                                .size(54.dp)
                                                .clip(CircleShape)
                                                .border(2.2.dp, ringBrush, CircleShape)
                                                .padding(3.dp),
                                            contentAlignment = Alignment.Center,
                                        ) {
                                            TelegramAvatar(idSeed = story.avatarSeed, title = story.authorName, size = 46.dp)
                                        }
                                        Spacer(Modifier.height(4.dp))
                                        Text(
                                            text = story.authorName,
                                            color = palette.topBarContent,
                                            fontSize = 11.sp,
                                            maxLines = 1,
                                            overflow = TextOverflow.Ellipsis,
                                        )
                                    }
                                }
                            }
                        }

                        LazyRow(
                            modifier = Modifier.fillMaxWidth(),
                            contentPadding = PaddingValues(horizontal = 10.dp),
                            horizontalArrangement = Arrangement.spacedBy(4.dp),
                        ) {
                            items(state.folderTabs, key = { it.id }) { tab ->
                                val isSelected = tab.id == state.selectedFolderId
                                val tabUnread = state.chats.count { TelegramFolderFilter.matches(tab, it) && (it.unreadCount > 0 || it.markedUnread) }
                                Column(
                                    horizontalAlignment = Alignment.CenterHorizontally,
                                    modifier = Modifier
                                        .clip(RoundedCornerShape(topStart = 8.dp, topEnd = 8.dp))
                                        .clickable { onSelectFolder(tab.id) }
                                        .padding(horizontal = 12.dp, vertical = 8.dp),
                                ) {
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Text(
                                            text = tab.title,
                                            color = if (isSelected) Color.White else palette.topBarSubtitle,
                                            fontSize = 14.sp,
                                            fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Medium,
                                        )
                                        if (tabUnread > 0) {
                                            Spacer(Modifier.width(5.dp))
                                            Box(
                                                modifier = Modifier
                                                    .clip(RoundedCornerShape(9.dp))
                                                    .background(if (isSelected) Color.White else Color.White.copy(alpha = 0.25f))
                                                    .padding(horizontal = 6.dp, vertical = 1.dp),
                                            ) {
                                                Text(
                                                    text = tabUnread.toString(),
                                                    color = if (isSelected) palette.topBarBackground else Color.White,
                                                    fontSize = 11.sp,
                                                    fontWeight = FontWeight.Bold,
                                                )
                                            }
                                        }
                                    }
                                    Spacer(Modifier.height(6.dp))
                                    Box(
                                        modifier = Modifier
                                            .width(28.dp)
                                            .height(3.dp)
                                            .clip(RoundedCornerShape(topStart = 3.dp, topEnd = 3.dp))
                                            .background(if (isSelected) Color.White else Color.Transparent),
                                    )
                                }
                            }
                            item(key = "manage-folders") {
                                Box(
                                    modifier = Modifier
                                        .padding(horizontal = 6.dp, vertical = 6.dp)
                                        .clip(RoundedCornerShape(10.dp))
                                        .clickable(onClick = onOpenFolderSettings)
                                        .padding(horizontal = 10.dp, vertical = 4.dp),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    TelegramIcon(TelegramGlyph.FOLDER, tint = palette.topBarSubtitle, size = 18.dp)
                                }
                            }
                        }
                    }
                }
            }

            LazyColumn(
                modifier = Modifier
                    .fillMaxSize()
                    .background(palette.surfaceColor)
                    .testTag("telegram-chat-list"),
                contentPadding = PaddingValues(bottom = 112.dp),
            ) {
                if (!state.globalSearchActive && state.selectedFolderId == "all" && archivedChats.isNotEmpty()) {
                    item(key = "archived-header") {
                        Column {
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .background(palette.elevatedSurface.copy(alpha = 0.55f))
                                    .clickable(onClick = onToggleArchiveExpanded)
                                    .padding(horizontal = 14.dp, vertical = 10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(46.dp)
                                        .clip(CircleShape)
                                        .background(palette.unreadBadgeMuted),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    TelegramIcon(TelegramGlyph.ARCHIVE, tint = Color.White, size = 22.dp)
                                }
                                Spacer(Modifier.width(12.dp))
                                Column(modifier = Modifier.weight(1f)) {
                                    Text("已归档对话", color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                                    Text(
                                        text = archivedChats.joinToString(" · ") { it.title },
                                        color = palette.secondaryText,
                                        fontSize = 13.sp,
                                        maxLines = 1,
                                        overflow = TextOverflow.Ellipsis,
                                    )
                                }
                                Box(
                                    modifier = Modifier
                                        .clip(RoundedCornerShape(10.dp))
                                        .background(palette.unreadBadgeMuted)
                                        .padding(horizontal = 8.dp, vertical = 2.dp),
                                ) {
                                    Text("${archivedChats.size} ${if (state.archiveExpanded) "▴" else "▾"}", color = Color.White, fontSize = 12.sp)
                                }
                            }
                            TelegramHairlineDivider()
                        }
                    }
                    if (state.archiveExpanded) {
                        items(archivedChats, key = { "arch-${it.id}" }) { archivedChat ->
                            SwipeableTelegramChatRow(
                                chat = archivedChat,
                                selected = archivedChat.id in state.selectedChatIds,
                                selectionMode = state.selectedChatIds.isNotEmpty(),
                                onClick = {
                                    if (state.selectedChatIds.isNotEmpty()) onToggleChatSelection(archivedChat.id)
                                    else onOpenChat(archivedChat.id)
                                },
                                onLongClick = { onToggleChatSelection(archivedChat.id) },
                                onSwipeArchive = { onArchiveChat(archivedChat.id) },
                                onSwipePin = { onPinChat(archivedChat.id) },
                            )
                        }
                    }
                }

                if (!state.globalSearchActive || state.globalSearchFilter == GlobalSearchFilter.CHATS) {
                    items(visibleChats, key = { it.id }) { chat ->
                        SwipeableTelegramChatRow(
                            chat = chat,
                            selected = chat.id in state.selectedChatIds,
                            selectionMode = state.selectedChatIds.isNotEmpty(),
                            onClick = {
                                if (state.selectedChatIds.isNotEmpty()) onToggleChatSelection(chat.id)
                                else onOpenChat(chat.id)
                            },
                            onLongClick = { onToggleChatSelection(chat.id) },
                            onSwipeArchive = { onArchiveChat(chat.id) },
                            onSwipePin = { onPinChat(chat.id) },
                        )
                    }
                }

                if (state.globalSearchActive && globalMessageMatches.isNotEmpty()) {
                    item(key = "global-msg-header") {
                        Text(
                            text = "全局消息与内容检索 (${globalMessageMatches.size})",
                            color = palette.primaryAccent,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier
                                .fillMaxWidth()
                                .background(palette.elevatedSurface)
                                .padding(horizontal = 16.dp, vertical = 8.dp),
                        )
                    }
                    items(globalMessageMatches, key = { "fts-${it.id}" }) { msg ->
                        Column(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { onOpenChat(msg.conversationId) }
                                .padding(horizontal = 16.dp, vertical = 10.dp),
                        ) {
                            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text(msg.senderName, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp)
                                Text(msg.time, color = palette.hintText, fontSize = 12.sp)
                            }
                            Spacer(Modifier.height(2.dp))
                            Text(msg.text, color = palette.secondaryText, fontSize = 13.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                        TelegramHairlineDivider()
                    }
                }

                if (visibleChats.isEmpty() && (!state.globalSearchActive || globalMessageMatches.isEmpty())) {
                    item(key = "empty-folder") {
                        Column(
                            modifier = Modifier.fillMaxWidth().padding(42.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            TelegramIcon(TelegramGlyph.FOLDER, tint = palette.hintText, size = 44.dp)
                            Spacer(Modifier.height(12.dp))
                            Text("当前筛选下暂无对话", color = palette.secondaryText, fontSize = 15.sp)
                        }
                    }
                }
            }
        }

        if (!state.globalSearchActive && state.selectedChatIds.isEmpty()) {
            Column(
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .navigationBarsPadding()
                    .padding(end = 16.dp, bottom = 18.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                SmallFloatingActionButton(
                    onClick = { showCreateStoryDialog = true },
                    containerColor = palette.elevatedSurface,
                    contentColor = palette.primaryText,
                    shape = CircleShape,
                ) {
                    TelegramIcon(TelegramGlyph.CAMERA, tint = palette.primaryAccent, size = 20.dp)
                }
                FloatingActionButton(
                    onClick = { showNewMessageSheet = true },
                    containerColor = palette.primaryAccent,
                    contentColor = Color.White,
                    shape = CircleShape,
                    modifier = Modifier.testTag("telegram-new-message-fab"),
                ) {
                    TelegramIcon(TelegramGlyph.PENCIL, tint = Color.White, size = 24.dp)
                }
            }
        }

        val activeStory = state.viewingStoryId?.let { id -> state.stories.firstOrNull { it.id == id } }
        if (activeStory != null) {
            TelegramStoryViewerOverlay(
                stories = state.stories,
                currentStory = activeStory,
                onSelectStory = onOpenStory,
                onClose = onCloseStory,
            )
        }

        TelegramBottomSheet(visible = showNewMessageSheet, onDismiss = { showNewMessageSheet = false }) {
            Text(
                "发起新消息",
                color = palette.primaryText,
                fontSize = 17.sp,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
            )
            NewChatSheetAction(TelegramGlyph.CONTACTS, "新建群组 (支持多话题 Forum Topics)") {
                showNewMessageSheet = false
                onRequestCreateKind(ConversationKind.GROUP)
            }
            NewChatSheetAction(TelegramGlyph.LOCK_CLOSED, "新建端到端加密聊天 (X3DH + Double Ratchet)", palette.secretGreen) {
                showNewMessageSheet = false
                onRequestCreateKind(ConversationKind.SECRET)
            }
            NewChatSheetAction(TelegramGlyph.SEND_PLANE, "新建公开或私密广播频道") {
                showNewMessageSheet = false
                onRequestCreateKind(ConversationKind.CHANNEL)
            }
            NewChatSheetAction(TelegramGlyph.MIKA_PHONE, "向 @MikaPhoneBot 发送手机操控任务", Color(0xFF00BFA5)) {
                showNewMessageSheet = false
                onOpenChat("bot-mika-phone")
            }
        }
    }

    if (showCreateStoryDialog) {
        CreateStoryDialog(
            onDismiss = { showCreateStoryDialog = false },
            onPublish = { headline, caption, grad ->
                onCreateStory(headline, caption, grad)
                showCreateStoryDialog = false
            },
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SwipeableTelegramChatRow(
    chat: TelegramChatItem,
    selected: Boolean,
    selectionMode: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    onSwipeArchive: () -> Unit,
    onSwipePin: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val haptic = LocalHapticFeedback.current
    var offsetX by remember(chat.id) { mutableFloatStateOf(0f) }

    Box(
        modifier = Modifier
            .fillMaxWidth()
            .background(
                when {
                    offsetX < -24f -> palette.primaryAccent.copy(alpha = 0.85f)
                    offsetX > 24f -> Color(0xFF4ECC5E).copy(alpha = 0.85f)
                    else -> palette.surfaceColor
                },
            ),
    ) {
        if (offsetX < -24f) {
            Row(
                modifier = Modifier.align(Alignment.CenterEnd).padding(end = 20.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(if (chat.isArchived) "移出归档" else "归档", color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Bold)
                Spacer(Modifier.width(8.dp))
                TelegramIcon(TelegramGlyph.ARCHIVE, tint = Color.White, size = 20.dp)
            }
        } else if (offsetX > 24f) {
            Row(
                modifier = Modifier.align(Alignment.CenterStart).padding(start = 20.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TelegramIcon(TelegramGlyph.PIN, tint = Color.White, size = 20.dp)
                Spacer(Modifier.width(8.dp))
                Text(if (chat.isPinned) "取消置顶" else "置顶", color = Color.White, fontSize = 13.sp, fontWeight = FontWeight.Bold)
            }
        }

        Column(
            modifier = Modifier
                .offset { IntOffset(offsetX.roundToInt(), 0) }
                .fillMaxWidth()
                .background(if (selected) palette.primaryAccent.copy(alpha = 0.12f) else palette.surfaceColor)
                .pointerInput(chat.id, selectionMode) {
                    if (selectionMode) return@pointerInput
                    detectHorizontalDragGestures(
                        onDragEnd = {
                            when {
                                offsetX <= -120f -> {
                                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                                    onSwipeArchive()
                                }
                                offsetX >= 120f -> {
                                    haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                                    onSwipePin()
                                }
                            }
                            offsetX = 0f
                        },
                        onDragCancel = { offsetX = 0f },
                        onHorizontalDrag = { _, dragAmount ->
                            offsetX = (offsetX + dragAmount).coerceIn(-180f, 180f)
                        },
                    )
                }
                .combinedClickable(
                    onClick = onClick,
                    onLongClick = {
                        haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                        onLongClick()
                    },
                ),
        ) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 14.dp, vertical = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box {
                    TelegramAvatar(
                        idSeed = chat.id,
                        title = chat.title,
                        size = 54.dp,
                        kind = chat.kind,
                        online = chat.online,
                        isSecret = chat.isSecretChat,
                        isCompanionGhost = chat.isCompanionGhost,
                    )
                    if (selected) {
                        Box(
                            modifier = Modifier
                                .align(Alignment.BottomEnd)
                                .size(22.dp)
                                .clip(CircleShape)
                                .background(palette.primaryAccent)
                                .border(2.dp, palette.surfaceColor, CircleShape),
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.CHECK_SINGLE, tint = Color.White, size = 13.dp)
                        }
                    }
                }

                Spacer(Modifier.width(12.dp))

                Column(modifier = Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        if (chat.isSecretChat) {
                            TelegramIcon(TelegramGlyph.LOCK_CLOSED, tint = palette.secretGreen, size = 15.dp)
                            Spacer(Modifier.width(4.dp))
                        }
                        Text(
                            text = chat.title.removePrefix("🔒 "),
                            color = if (chat.isSecretChat) palette.secretGreen else palette.primaryText,
                            fontSize = 16.sp,
                            fontWeight = FontWeight.SemiBold,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f, fill = false),
                        )
                        if (chat.isVerified) {
                            Spacer(Modifier.width(5.dp))
                            TelegramIcon(TelegramGlyph.VERIFIED, tint = palette.primaryAccent, size = 15.dp)
                        }
                        if (chat.isMuted) {
                            Spacer(Modifier.width(5.dp))
                            TelegramIcon(TelegramGlyph.MUTE, tint = palette.hintText, size = 14.dp)
                        }
                        Spacer(Modifier.weight(1f))
                        if (chat.lastMessageOutgoing) {
                            TelegramIcon(
                                glyph = if (chat.lastDeliveryState == "read") TelegramGlyph.CHECK_DOUBLE else TelegramGlyph.CHECK_SINGLE,
                                tint = palette.checkMarkColor,
                                size = 15.dp,
                            )
                            Spacer(Modifier.width(4.dp))
                        }
                        Text(chat.time, color = palette.hintText, fontSize = 12.sp)
                    }

                    Spacer(Modifier.height(4.dp))

                    Row(verticalAlignment = Alignment.CenterVertically) {
                        val previewAnnotated = buildAnnotatedString {
                            when {
                                chat.typingLabel != null -> {
                                    withStyle(SpanStyle(color = palette.primaryAccent, fontWeight = FontWeight.Medium)) {
                                        append(chat.typingLabel)
                                    }
                                }
                                chat.draftText.isNotBlank() -> {
                                    withStyle(SpanStyle(color = palette.destructiveRed, fontWeight = FontWeight.Medium)) {
                                        append("草稿: ")
                                    }
                                    withStyle(SpanStyle(color = palette.secondaryText)) {
                                        append(chat.draftText)
                                    }
                                }
                                else -> {
                                    if (chat.isForum) {
                                        withStyle(SpanStyle(color = palette.primaryAccent, fontWeight = FontWeight.Medium)) {
                                            append("#话题 · ")
                                        }
                                    }
                                    withStyle(SpanStyle(color = palette.secondaryText)) {
                                        append(chat.preview)
                                    }
                                }
                            }
                        }
                        Text(
                            text = previewAnnotated,
                            fontSize = 14.sp,
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                            modifier = Modifier.weight(1f),
                        )

                        Spacer(Modifier.width(8.dp))

                        if (chat.mentionCount > 0) {
                            Box(
                                modifier = Modifier
                                    .size(22.dp)
                                    .clip(CircleShape)
                                    .background(palette.unreadBadgeActive),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text("@", color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                            }
                            Spacer(Modifier.width(4.dp))
                        }

                        if (chat.unreadCount > 0 || chat.markedUnread) {
                            val label = if (chat.unreadCount > 0) chat.unreadCount.toString() else "•"
                            Box(
                                modifier = Modifier
                                    .clip(RoundedCornerShape(12.dp))
                                    .background(if (chat.isMuted) palette.unreadBadgeMuted else palette.unreadBadgeActive)
                                    .padding(horizontal = 7.dp, vertical = 2.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(label, color = Color.White, fontSize = 12.sp, fontWeight = FontWeight.Bold)
                            }
                        } else if (chat.isPinned) {
                            TelegramIcon(TelegramGlyph.PIN, tint = palette.hintText, size = 16.dp)
                        }
                    }
                }
            }
            TelegramHairlineDivider(Modifier.padding(start = 80.dp))
        }
    }
}

@Composable
private fun NewChatSheetAction(
    glyph: TelegramGlyph,
    title: String,
    tint: Color? = null,
    onClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 20.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TelegramIcon(glyph, tint = tint ?: palette.primaryAccent, size = 22.dp)
        Spacer(Modifier.width(16.dp))
        Text(title, color = palette.primaryText, fontSize = 15.sp, fontWeight = FontWeight.Medium)
    }
}

@Composable
private fun IconButtonCircle(
    glyph: TelegramGlyph,
    tint: Color,
    testTag: String? = null,
    onClick: () -> Unit,
) {
    Box(
        modifier = Modifier
            .size(40.dp)
            .clip(CircleShape)
            .let { if (testTag != null) it.testTag(testTag) else it }
            .clickable(onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        TelegramIcon(glyph = glyph, tint = tint, size = 21.dp)
    }
}

@Composable
private fun TelegramStoryViewerOverlay(
    stories: List<TelegramStoryItem>,
    currentStory: TelegramStoryItem,
    onSelectStory: (String) -> Unit,
    onClose: () -> Unit,
) {
    BackHandler(onBack = onClose)
    var progress by remember(currentStory.id) { mutableFloatStateOf(0f) }
    val currentIndex = stories.indexOfFirst { it.id == currentStory.id }.coerceAtLeast(0)

    LaunchedEffect(currentStory.id) {
        progress = 0f
        repeat(50) {
            delay(100)
            progress = ((it + 1) / 50f).coerceIn(0f, 1f)
        }
        if (currentIndex + 1 < stories.size) {
            onSelectStory(stories[currentIndex + 1].id)
        } else {
            onClose()
        }
    }

    val bgStops = when (currentStory.gradientIndex % 4) {
        0 -> listOf(Color(0xFF1A2980), Color(0xFF26D0CE))
        1 -> listOf(Color(0xFF134E5E), Color(0xFF71B280))
        2 -> listOf(Color(0xFF42275A), Color(0xFFD99B26))
        else -> listOf(Color(0xFF0F2027), Color(0xFF2C5364))
    }

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Brush.verticalGradient(bgStops))
            .statusBarsPadding()
            .navigationBarsPadding(),
    ) {
        Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                stories.forEachIndexed { idx, _ ->
                    val segProgress = when {
                        idx < currentIndex -> 1f
                        idx == currentIndex -> progress
                        else -> 0f
                    }
                    LinearProgressIndicator(
                        progress = { segProgress },
                        modifier = Modifier.weight(1f).height(3.dp).clip(RoundedCornerShape(2.dp)),
                        color = Color.White,
                        trackColor = Color.White.copy(alpha = 0.30f),
                    )
                }
            }
            Spacer(Modifier.height(12.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                TelegramAvatar(idSeed = currentStory.avatarSeed, title = currentStory.authorName, size = 38.dp)
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text(currentStory.authorName, color = Color.White, fontWeight = FontWeight.Bold, fontSize = 15.sp)
                    Text("👁 ${currentStory.viewCount} 次浏览 · 24h 动态", color = Color.White.copy(alpha = 0.78f), fontSize = 12.sp)
                }
                IconButtonCircle(TelegramGlyph.CLOSE, Color.White, onClick = onClose)
            }

            Box(
                modifier = Modifier
                    .weight(1f)
                    .fillMaxWidth()
                    .clickable {
                        if (currentIndex + 1 < stories.size) onSelectStory(stories[currentIndex + 1].id) else onClose()
                    },
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.padding(24.dp)) {
                    Text("✦", color = Color(0xFFFFD54F), fontSize = 42.sp)
                    Spacer(Modifier.height(16.dp))
                    Text(currentStory.headline, color = Color.White, fontSize = 26.sp, fontWeight = FontWeight.ExtraBold)
                    Spacer(Modifier.height(14.dp))
                    Text(currentStory.caption, color = Color.White.copy(alpha = 0.92f), fontSize = 16.sp)
                }
            }
        }
    }
}

@Composable
private fun CreateStoryDialog(
    onDismiss: () -> Unit,
    onPublish: (String, String, Int) -> Unit,
) {
    var headline by remember { mutableStateOf("") }
    var caption by remember { mutableStateOf("") }
    var gradIdx by remember { mutableIntStateOf(0) }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("发布 24 小时快拍动态 (Story)") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(value = headline, onValueChange = { headline = it }, label = { Text("大标题") }, singleLine = true)
                OutlinedTextField(value = caption, onValueChange = { caption = it }, label = { Text("动态正文说明") })
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf("深海蓝", "翡翠绿", "琉璃金", "星空夜").forEachIndexed { idx, label ->
                        TextButton(onClick = { gradIdx = idx }) {
                            Text(if (gradIdx == idx) "● $label" else label)
                        }
                    }
                }
            }
        },
        confirmButton = {
            Button(onClick = { onPublish(headline, caption, gradIdx) }) { Text("立即发布") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("取消") }
        },
    )
}

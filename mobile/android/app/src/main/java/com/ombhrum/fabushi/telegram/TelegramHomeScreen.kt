package com.ombhrum.fabushi.telegram

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ombhrum.fabushi.ConversationKind
import kotlin.math.roundToInt

/**
 * Apple iOS Messages Home Screen (`F13`, Master Spec §8.8):
 * - Zero Material FABs, zero Drawer hamburger button, zero raw emojis, zero duplicate search icons
 * - Large iOS title (`信息` / `Messages`), top-left `编辑` bulk edit mode (operating on ALL selected chats),
 *   top-right iOS Compose square-and-pen button (always accessible for Add Contact / DM / Group / Channel / Secret / Bot)
 * - Single inline rounded `10dp` search pill wired to real `serverSearchResults` + local chat matches
 * - Authentic 3-column circular Pinned Chats avatar grid powered by `partitionPinnedChatsGrid(chats)`
 * - Clean `68dp` iOS conversation rows with `0.5dp` indented hairlines and dp-density smooth swipe actions
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun TelegramHomeScreen(
    state: TelegramUiState,
    onSelectFolder: (String) -> Unit,
    onToggleArchiveExpanded: () -> Unit,
    onGlobalSearchChange: (Boolean, String, GlobalSearchFilter) -> Unit,
    onOpenChat: (String) -> Unit,
    onToggleSelectChat: (String) -> Unit,
    onClearSelection: () -> Unit,
    onTogglePinChat: (String) -> Unit,
    onToggleArchiveChat: (String) -> Unit,
    onToggleMuteChat: (String) -> Unit,
    onToggleUnreadChat: (String) -> Unit,
    onDeleteChat: (String) -> Unit,
    onCreateDialogKind: (ConversationKind) -> Unit,
    onOpenBotFather: () -> Unit,
    onAddContactByUsername: (String) -> Unit,
    onLockAppNow: () -> Unit,
    onRefreshChats: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val density = LocalDensity.current

    var editMode by remember { mutableStateOf(false) }
    var showComposeSheet by remember { mutableStateOf(false) }
    var showAddContactDialog by remember { mutableStateOf(false) }
    var addContactInput by remember { mutableStateOf("") }
    var pinnedContextChatId by remember { mutableStateOf<String?>(null) }

    val selectedTab = remember(state.folderTabs, state.selectedFolderId) {
        state.folderTabs.firstOrNull { it.id == state.selectedFolderId } ?: state.folderTabs.firstOrNull()
    }

    val nonArchivedChats = remember(state.chats, selectedTab) {
        state.chats.filter { ch ->
            !ch.isArchived && (selectedTab == null || TelegramFolderFilter.matches(selectedTab, ch))
        }
    }
    val archivedChats = remember(state.chats) { state.chats.filter { it.isArchived } }
    val pinnedLayout = remember(nonArchivedChats) { partitionPinnedChatsGrid(nonArchivedChats) }
    val unpinnedChats = remember(nonArchivedChats) { nonArchivedChats.filter { !it.isPinned } }

    // Search results merging local chats + real serverSearchResults + local cached messages
    val searchQuery = state.globalSearchQuery.trim()
    val matchedChats = remember(state.chats, searchQuery) {
        if (searchQuery.isBlank()) emptyList()
        else state.chats.filter {
            it.title.contains(searchQuery, ignoreCase = true) ||
                it.preview.contains(searchQuery, ignoreCase = true) ||
                (it.username?.contains(searchQuery, ignoreCase = true) == true)
        }
    }
    val matchedMessages = remember(state.serverSearchResults, state.messagesByChat, searchQuery) {
        if (searchQuery.isBlank()) emptyList()
        else {
            val combined = LinkedHashMap<String, TelegramMessageItem>()
            state.serverSearchResults.forEach { combined["${it.conversationId}:${it.id}"] = it }
            state.messagesByChat.values.flatten()
                .filter { it.text.contains(searchQuery, ignoreCase = true) }
                .forEach { combined.putIfAbsent("${it.conversationId}:${it.id}", it) }
            combined.values.toList()
        }
    }

    val isSelecting = editMode || state.selectedChatIds.isNotEmpty()

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-home-screen"),
    ) {
        Column(modifier = Modifier.fillMaxSize()) {
            // Apple iOS Top Navigation Bar (Edit | Title | Lock? + Compose)
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .background(palette.topBarBackground)
                    .statusBarsPadding()
                    .padding(horizontal = 16.dp, vertical = 8.dp),
            ) {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        text = if (isSelecting) strings.done else strings.edit,
                        color = palette.topBarActionColor,
                        fontSize = 16.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .clickable {
                                if (isSelecting) {
                                    editMode = false
                                    onClearSelection()
                                } else {
                                    editMode = true
                                }
                            }
                            .padding(horizontal = 4.dp, vertical = 4.dp)
                            .testTag("home-edit-button"),
                    )

                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(10.dp),
                    ) {
                        if (state.passcodeConfigured) {
                            Box(
                                modifier = Modifier
                                    .size(32.dp)
                                    .clip(CircleShape)
                                    .clickable(onClick = onLockAppNow)
                                    .testTag("telegram-passcode-lock-button"),
                                contentAlignment = Alignment.Center,
                            ) {
                                TelegramIcon(TelegramGlyph.LOCK_CLOSED, tint = palette.topBarActionColor, size = 18.dp)
                            }
                        }
                        Box(
                            modifier = Modifier
                                .size(34.dp)
                                .clip(CircleShape)
                                .clickable { showComposeSheet = true }
                                .testTag("home-compose-button"),
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.COMPOSE_SQUARE_PEN, tint = palette.topBarActionColor, size = 21.dp)
                        }
                    }
                }

                Spacer(Modifier.height(4.dp))

                // Large iOS Title + subtle real sync status pill
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Text(
                        text = strings.tabMessages,
                        color = palette.topBarContent,
                        fontSize = 32.sp,
                        fontWeight = FontWeight.Bold,
                    )
                    if (state.syncStatusLabel == "同步中…" || !state.serverWsConnected && state.isAuthenticated) {
                        Surface(
                            shape = RoundedCornerShape(10.dp),
                            color = palette.inputBg,
                            modifier = Modifier.clickable(onClick = onRefreshChats),
                        ) {
                            Text(
                                text = state.syncStatusLabel.ifBlank { if (strings.isEnglish) "Syncing" else "连接中" },
                                color = palette.secondaryText,
                                fontSize = 11.sp,
                                modifier = Modifier.padding(horizontal = 8.dp, vertical = 3.dp),
                            )
                        }
                    }
                }

                Spacer(Modifier.height(10.dp))

                // Single Inline Rounded Pill Search Bar (10dp radius, contrast-safe cursor!)
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(38.dp)
                        .clip(RoundedCornerShape(10.dp))
                        .background(palette.inputBg)
                        .padding(horizontal = 10.dp)
                        .testTag("apple-inline-search-pill"),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    TelegramIcon(TelegramGlyph.SEARCH, tint = palette.secondaryText, size = 16.dp)
                    Spacer(Modifier.width(8.dp))
                    Box(modifier = Modifier.weight(1f), contentAlignment = Alignment.CenterStart) {
                        if (state.globalSearchQuery.isEmpty()) {
                            Text(
                                text = strings.searchPlaceholder,
                                color = palette.secondaryText,
                                fontSize = 15.sp,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                        BasicTextField(
                            value = state.globalSearchQuery,
                            onValueChange = { q ->
                                onGlobalSearchChange(q.isNotEmpty(), q, state.globalSearchFilter)
                            },
                            singleLine = true,
                            textStyle = TextStyle(
                                color = palette.primaryText,
                                fontSize = 15.sp,
                            ),
                            cursorBrush = SolidColor(palette.searchCursorColor),
                            modifier = Modifier
                                .fillMaxWidth()
                                .testTag("telegram-global-search-input"),
                        )
                    }
                    if (state.globalSearchQuery.isNotEmpty()) {
                        Box(
                            modifier = Modifier
                                .size(22.dp)
                                .clip(CircleShape)
                                .background(palette.hintText.copy(alpha = 0.35f))
                                .clickable { onGlobalSearchChange(false, "", GlobalSearchFilter.CHATS) },
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(TelegramGlyph.CLOSE, tint = palette.primaryText, size = 12.dp)
                        }
                    }
                }

                // Folder Filter Pills (only when not searching and >1 tabs exist)
                if (!state.globalSearchActive && state.folderTabs.size > 1) {
                    Spacer(Modifier.height(10.dp))
                    LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        itemsIndexed(state.folderTabs, key = { index, tab -> "folder_${index}_${tab.id}" }) { _, tab ->
                            val selected = tab.id == state.selectedFolderId
                            Surface(
                                shape = RoundedCornerShape(16.dp),
                                color = if (selected) palette.selectedPillBackground else palette.inputBg,
                                modifier = Modifier
                                    .clip(RoundedCornerShape(16.dp))
                                    .clickable { onSelectFolder(tab.id) },
                            ) {
                                Text(
                                    text = tab.title,
                                    color = if (selected) palette.selectedPillContent else palette.primaryText,
                                    fontSize = 13.sp,
                                    fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
                                    modifier = Modifier.padding(horizontal = 12.dp, vertical = 6.dp),
                                )
                            }
                        }
                    }
                }

                // Search Filter Pills when searching (fixed APPLE_DAY contrast!)
                if (state.globalSearchActive) {
                    Spacer(Modifier.height(8.dp))
                    LazyRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                        items(GlobalSearchFilter.entries) { filter ->
                            val selected = filter == state.globalSearchFilter
                            Surface(
                                shape = RoundedCornerShape(14.dp),
                                color = if (selected) palette.selectedPillBackground else palette.inputBg,
                                modifier = Modifier
                                    .clip(RoundedCornerShape(14.dp))
                                    .clickable { onGlobalSearchChange(true, state.globalSearchQuery, filter) },
                            ) {
                                Text(
                                    text = filter.label,
                                    color = if (selected) palette.selectedPillContent else palette.primaryText,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Medium,
                                    modifier = Modifier.padding(horizontal = 11.dp, vertical = 5.dp),
                                )
                            }
                        }
                    }
                }
            }

            // Bulk Selection Action Bar (operates on ALL selectedChatIds; scrollable & compact for 360dp screens)
            if (state.selectedChatIds.isNotEmpty()) {
                val selectedChats = state.selectedChatIds.mapNotNull { id ->
                    state.chats.firstOrNull { it.id == id }
                }
                val unreadSelectedIds = selectedChats
                    .filter { it.unreadCount > 0 || it.markedUnread }
                    .map { it.id }
                val bulkReadLabel = if (unreadSelectedIds.isNotEmpty() || selectedChats.isEmpty()) {
                    strings.markReadAction
                } else {
                    strings.markUnreadAction
                }
                val compactButtonPadding = PaddingValues(horizontal = 8.dp, vertical = 4.dp)
                val compactButtonMod = Modifier.defaultMinSize(minWidth = 1.dp, minHeight = 36.dp)

                Surface(
                    color = palette.cardBg,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(horizontal = 12.dp, vertical = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = if (strings.isEnglish) "${state.selectedChatIds.size} selected" else "已选 ${state.selectedChatIds.size} 项",
                            color = palette.primaryText,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.SemiBold,
                            maxLines = 1,
                        )
                        Spacer(Modifier.width(8.dp))
                        Row(
                            modifier = Modifier
                                .weight(1f)
                                .horizontalScroll(rememberScrollState()),
                            horizontalArrangement = Arrangement.spacedBy(2.dp, Alignment.End),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            TextButton(
                                onClick = {
                                    state.selectedChatIds.toList().forEach(onTogglePinChat)
                                    onClearSelection()
                                    editMode = false
                                },
                                contentPadding = compactButtonPadding,
                                modifier = compactButtonMod,
                            ) { Text(strings.pinAction, color = palette.topBarActionColor, fontSize = 12.sp, maxLines = 1) }
                            TextButton(
                                onClick = {
                                    state.selectedChatIds.toList().forEach(onToggleMuteChat)
                                    onClearSelection()
                                    editMode = false
                                },
                                contentPadding = compactButtonPadding,
                                modifier = compactButtonMod,
                            ) { Text(strings.muteAction, color = palette.topBarActionColor, fontSize = 12.sp, maxLines = 1) }
                            TextButton(
                                onClick = {
                                    val targets = unreadSelectedIds.ifEmpty { state.selectedChatIds.toList() }
                                    targets.forEach(onToggleUnreadChat)
                                    onClearSelection()
                                    editMode = false
                                },
                                contentPadding = compactButtonPadding,
                                modifier = compactButtonMod,
                            ) { Text(bulkReadLabel, color = palette.topBarActionColor, fontSize = 12.sp, maxLines = 1) }
                            TextButton(
                                onClick = {
                                    state.selectedChatIds.toList().forEach(onToggleArchiveChat)
                                    onClearSelection()
                                    editMode = false
                                },
                                contentPadding = compactButtonPadding,
                                modifier = compactButtonMod,
                            ) { Text(strings.archiveAction, color = palette.topBarActionColor, fontSize = 12.sp, maxLines = 1) }
                            TextButton(
                                onClick = {
                                    state.selectedChatIds.toList().forEach(onDeleteChat)
                                    onClearSelection()
                                    editMode = false
                                },
                                contentPadding = compactButtonPadding,
                                modifier = compactButtonMod,
                            ) { Text(strings.delete, color = palette.destructiveRed, fontSize = 12.sp, maxLines = 1) }
                        }
                    }
                }
            }

            TelegramHairlineDivider()

            // Main Content: Global Search Results OR Pinned Grid + Conversation List
            if (state.globalSearchActive && searchQuery.isNotBlank()) {
                LazyColumn(
                    modifier = Modifier.weight(1f).fillMaxWidth(),
                    contentPadding = PaddingValues(vertical = 8.dp),
                ) {
                    if (matchedChats.isNotEmpty()) {
                        item {
                            Text(
                                text = if (strings.isEnglish) "Conversations" else "匹配的对话",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.SemiBold,
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                            )
                        }
                        items(matchedChats, key = { "search-chat-${it.id}" }) { chat ->
                            AppleChatListRow(
                                chat = chat,
                                isEditMode = false,
                                isSelected = false,
                                onClick = { onOpenChat(chat.id) },
                                onLongClick = {},
                            )
                            TelegramHairlineDivider(Modifier.padding(start = 80.dp))
                        }
                    }

                    if (matchedMessages.isNotEmpty()) {
                        item {
                            Text(
                                text = if (strings.isEnglish) "Messages (${matchedMessages.size})" else "服务端与本地消息 (${matchedMessages.size})",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                                fontWeight = FontWeight.SemiBold,
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            )
                        }
                        items(matchedMessages, key = { "search-msg-${it.conversationId}-${it.id}" }) { msg ->
                            val chatTitle = state.chats.firstOrNull { it.id == msg.conversationId }?.title ?: msg.senderName
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clickable { onOpenChat(msg.conversationId) }
                                    .padding(horizontal = 16.dp, vertical = 10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Column(Modifier.weight(1f)) {
                                    Row(
                                        modifier = Modifier.fillMaxWidth(),
                                        horizontalArrangement = Arrangement.SpaceBetween,
                                    ) {
                                        Text(chatTitle, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 14.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                        Text(msg.time, color = palette.secondaryText, fontSize = 12.sp)
                                    }
                                    Spacer(Modifier.height(2.dp))
                                    Text(msg.text, color = palette.secondaryText, fontSize = 13.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                }
                            }
                            TelegramHairlineDivider(Modifier.padding(start = 16.dp))
                        }
                    }

                    if (matchedChats.isEmpty() && matchedMessages.isEmpty()) {
                        item {
                            Box(
                                modifier = Modifier.fillMaxWidth().padding(40.dp),
                                contentAlignment = Alignment.Center,
                            ) {
                                Text(
                                    text = if (strings.isEnglish) "No matching conversations or messages." else "未找到匹配的对话或消息",
                                    color = palette.secondaryText,
                                    fontSize = 14.sp,
                                )
                            }
                        }
                    }
                }
            } else {
                LazyColumn(
                    modifier = Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .testTag("telegram-chat-list"),
                ) {
                    // Authentic 3-Column Circular Pinned Chats Grid (`partitionPinnedChatsGrid`)
                    if (pinnedLayout.rows.isNotEmpty()) {
                        item(key = "apple-pinned-grid") {
                            Column(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(horizontal = 16.dp, vertical = 14.dp)
                                    .testTag("apple-pinned-chats-grid"),
                                verticalArrangement = Arrangement.spacedBy(14.dp),
                            ) {
                                pinnedLayout.rows.forEach { rowChats ->
                                    if (pinnedLayout.centeredSingleRow) {
                                        Row(
                                            modifier = Modifier.fillMaxWidth(),
                                            horizontalArrangement = Arrangement.spacedBy(24.dp, Alignment.CenterHorizontally),
                                        ) {
                                            rowChats.forEach { chat ->
                                                PinnedAvatarCell(
                                                    chat = chat,
                                                    avatarSize = 82.dp,
                                                    menuExpanded = pinnedContextChatId == chat.id,
                                                    onDismissMenu = { pinnedContextChatId = null },
                                                    onClick = {
                                                        if (isSelecting) onToggleSelectChat(chat.id)
                                                        else onOpenChat(chat.id)
                                                    },
                                                    onLongClick = { pinnedContextChatId = chat.id },
                                                    onUnpin = {
                                                        pinnedContextChatId = null
                                                        onTogglePinChat(chat.id)
                                                    },
                                                    onMute = {
                                                        pinnedContextChatId = null
                                                        onToggleMuteChat(chat.id)
                                                    },
                                                    onArchive = {
                                                        pinnedContextChatId = null
                                                        onToggleArchiveChat(chat.id)
                                                    },
                                                    onDelete = {
                                                        pinnedContextChatId = null
                                                        onDeleteChat(chat.id)
                                                    },
                                                )
                                            }
                                        }
                                    } else {
                                        Row(
                                            modifier = Modifier.fillMaxWidth(),
                                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                                        ) {
                                            rowChats.forEach { chat ->
                                                Box(modifier = Modifier.weight(1f), contentAlignment = Alignment.Center) {
                                                    PinnedAvatarCell(
                                                        chat = chat,
                                                        avatarSize = 72.dp,
                                                        menuExpanded = pinnedContextChatId == chat.id,
                                                        onDismissMenu = { pinnedContextChatId = null },
                                                        onClick = {
                                                            if (isSelecting) onToggleSelectChat(chat.id)
                                                            else onOpenChat(chat.id)
                                                        },
                                                        onLongClick = { pinnedContextChatId = chat.id },
                                                        onUnpin = {
                                                            pinnedContextChatId = null
                                                            onTogglePinChat(chat.id)
                                                        },
                                                        onMute = {
                                                            pinnedContextChatId = null
                                                            onToggleMuteChat(chat.id)
                                                        },
                                                        onArchive = {
                                                            pinnedContextChatId = null
                                                            onToggleArchiveChat(chat.id)
                                                        },
                                                        onDelete = {
                                                            pinnedContextChatId = null
                                                            onDeleteChat(chat.id)
                                                        },
                                                    )
                                                }
                                            }
                                            repeat(3 - rowChats.size) {
                                                Spacer(Modifier.weight(1f))
                                            }
                                        }
                                    }
                                }
                            }
                            TelegramHairlineDivider()
                        }
                    }

                    // Archived chats collapsible row
                    if (archivedChats.isNotEmpty()) {
                        item(key = "archived-header") {
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .clickable(onClick = onToggleArchiveExpanded)
                                    .padding(horizontal = 16.dp, vertical = 12.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(44.dp)
                                        .clip(CircleShape)
                                        .background(palette.inputBg),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    TelegramIcon(TelegramGlyph.ARCHIVE, tint = palette.secondaryText, size = 20.dp)
                                }
                                Spacer(Modifier.width(12.dp))
                                Text(
                                    text = if (strings.isEnglish) "Archived (${archivedChats.size})" else "已归档对话 (${archivedChats.size})",
                                    color = palette.primaryText,
                                    fontWeight = FontWeight.Medium,
                                    fontSize = 15.sp,
                                    modifier = Modifier.weight(1f),
                                )
                                TelegramIcon(
                                    glyph = if (state.archiveExpanded) TelegramGlyph.CHEVRON_DOWN else TelegramGlyph.CHEVRON_RIGHT,
                                    tint = palette.hintText,
                                    size = 15.dp,
                                )
                            }
                            TelegramHairlineDivider(Modifier.padding(start = 72.dp))
                        }
                        if (state.archiveExpanded) {
                            items(archivedChats, key = { "arch-${it.id}" }) { chat ->
                                SwipeableAppleChatRow(
                                    chat = chat,
                                    densityPxThreshold = with(density) { 64.dp.toPx() },
                                    isEditMode = isSelecting,
                                    isSelected = chat.id in state.selectedChatIds,
                                    onClick = {
                                        if (isSelecting) onToggleSelectChat(chat.id) else onOpenChat(chat.id)
                                    },
                                    onLongClick = { onToggleSelectChat(chat.id) },
                                    onPin = { onTogglePinChat(chat.id) },
                                    onArchive = { onToggleArchiveChat(chat.id) },
                                )
                                TelegramHairlineDivider(Modifier.padding(start = 80.dp))
                            }
                        }
                    }

                    // Unpinned Conversations List
                    if (nonArchivedChats.isEmpty() && archivedChats.isEmpty()) {
                        item(key = "empty-chats") {
                            Column(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .padding(horizontal = 32.dp, vertical = 64.dp)
                                    .testTag("telegram-empty-chats-state"),
                                horizontalAlignment = Alignment.CenterHorizontally,
                                verticalArrangement = Arrangement.spacedBy(12.dp),
                            ) {
                                Box(
                                    modifier = Modifier
                                        .size(64.dp)
                                        .clip(CircleShape)
                                        .background(palette.inputBg),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    TelegramIcon(TelegramGlyph.CHAT_BUBBLE, tint = palette.primaryAccent, size = 30.dp)
                                }
                                Text(
                                    text = strings.noChatsTitle,
                                    color = palette.primaryText,
                                    fontSize = 18.sp,
                                    fontWeight = FontWeight.SemiBold,
                                )
                                Text(
                                    text = strings.noChatsSubtitle,
                                    color = palette.secondaryText,
                                    fontSize = 13.sp,
                                    textAlign = TextAlign.Center,
                                )
                                Button(
                                    onClick = { showComposeSheet = true },
                                    colors = ButtonDefaults.buttonColors(
                                        containerColor = palette.primaryAccent,
                                        contentColor = Color.White,
                                    ),
                                    shape = RoundedCornerShape(12.dp),
                                ) {
                                    Text(strings.composeTitle)
                                }
                            }
                        }
                    } else {
                        items(unpinnedChats, key = { it.id }) { chat ->
                            SwipeableAppleChatRow(
                                chat = chat,
                                densityPxThreshold = with(density) { 64.dp.toPx() },
                                isEditMode = isSelecting,
                                isSelected = chat.id in state.selectedChatIds,
                                onClick = {
                                    if (isSelecting) onToggleSelectChat(chat.id) else onOpenChat(chat.id)
                                },
                                onLongClick = { onToggleSelectChat(chat.id) },
                                onPin = { onTogglePinChat(chat.id) },
                                onArchive = { onToggleArchiveChat(chat.id) },
                            )
                            TelegramHairlineDivider(Modifier.padding(start = 80.dp))
                        }
                    }
                }
            }
        }

        // Apple Compose Bottom Sheet (always accessible from top-right Compose button!)
        TelegramBottomSheet(visible = showComposeSheet, onDismiss = { showComposeSheet = false }) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 16.dp, vertical = 6.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(
                    text = strings.composeTitle,
                    color = palette.primaryText,
                    fontSize = 17.sp,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.padding(horizontal = 6.dp, vertical = 6.dp),
                )
                ComposeSheetRow(
                    glyph = TelegramGlyph.CONTACTS,
                    badgeColor = palette.primaryAccent,
                    title = strings.addContactAction,
                    onClick = {
                        showComposeSheet = false
                        showAddContactDialog = true
                    },
                )
                ComposeSheetRow(
                    glyph = TelegramGlyph.CHAT_BUBBLE,
                    badgeColor = Color(0xFF34C759),
                    title = strings.newGroupAction,
                    onClick = {
                        showComposeSheet = false
                        onCreateDialogKind(ConversationKind.GROUP)
                    },
                )
                ComposeSheetRow(
                    glyph = TelegramGlyph.COMPOSE_SQUARE_PEN,
                    badgeColor = Color(0xFFFF9500),
                    title = strings.newChannelAction,
                    onClick = {
                        showComposeSheet = false
                        onCreateDialogKind(ConversationKind.CHANNEL)
                    },
                )
                ComposeSheetRow(
                    glyph = TelegramGlyph.SECRET_SHIELD,
                    badgeColor = Color(0xFF30B0C7),
                    title = strings.newSecretChatAction,
                    onClick = {
                        showComposeSheet = false
                        onCreateDialogKind(ConversationKind.SECRET)
                    },
                )
                ComposeSheetRow(
                    glyph = TelegramGlyph.BOT_ROBOT,
                    badgeColor = Color(0xFFAF52DE),
                    title = strings.newBotAction,
                    onClick = {
                        showComposeSheet = false
                        onOpenBotFather()
                    },
                )
            }
        }

        // Add Contact / Start Direct Chat Dialog
        if (showAddContactDialog) {
            AlertDialog(
                onDismissRequest = { showAddContactDialog = false },
                containerColor = palette.modalBg,
                titleContentColor = palette.primaryText,
                textContentColor = palette.primaryText,
                title = { Text(strings.addContactAction) },
                text = {
                    OutlinedTextField(
                        value = addContactInput,
                        onValueChange = { addContactInput = it },
                        label = { Text(if (strings.isEnglish) "@username or phone" else "对方 @用户名或手机号") },
                        singleLine = true,
                        colors = OutlinedTextFieldDefaults.colors(
                            focusedTextColor = palette.primaryText,
                            unfocusedTextColor = palette.primaryText,
                            cursorColor = palette.searchCursorColor,
                        ),
                        modifier = Modifier.fillMaxWidth(),
                    )
                },
                confirmButton = {
                    Button(
                        enabled = addContactInput.isNotBlank(),
                        onClick = {
                            onAddContactByUsername(addContactInput.trim())
                            addContactInput = ""
                            showAddContactDialog = false
                        },
                        colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                    ) {
                        Text(if (strings.isEnglish) "Add & Chat" else "添加并聊天")
                    }
                },
                dismissButton = {
                    TextButton(onClick = { showAddContactDialog = false }) {
                        Text(strings.cancel, color = palette.topBarActionColor)
                    }
                },
            )
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PinnedAvatarCell(
    chat: TelegramChatItem,
    avatarSize: androidx.compose.ui.unit.Dp,
    menuExpanded: Boolean,
    onDismissMenu: () -> Unit,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    onUnpin: () -> Unit,
    onMute: () -> Unit,
    onArchive: () -> Unit,
    onDelete: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val liveBotStatus = formatBotActivitySubtitle(
        chat.botExecutionState,
        chat.botActivityDetail,
        if (strings.isEnglish) "en-US" else "zh-CN",
    )

    Column(
        modifier = Modifier
            .width(92.dp)
            .clip(RoundedCornerShape(14.dp))
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(vertical = 4.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Box {
            TelegramAvatar(
                idSeed = chat.id,
                title = chat.title,
                size = avatarSize,
                kind = chat.kind,
                online = chat.online,
                isSecret = chat.isSecretChat,
                isCompanionGhost = chat.isCompanionGhost && !chat.isBot,
                avatarShape = chat.avatarShape,
                avatarColor = chat.avatarColor,
                isBot = chat.isBot,
                botActivityActive = liveBotStatus != null,
            )
            if (chat.unreadCount > 0 || chat.markedUnread) {
                Box(
                    modifier = Modifier
                        .align(Alignment.TopEnd)
                        .clip(CircleShape)
                        .background(if (chat.isMuted) palette.unreadBadgeMuted else palette.unreadBadgeActive)
                        .border(2.dp, palette.appBackground, CircleShape)
                        .padding(horizontal = 6.dp, vertical = 2.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Text(
                        text = if (chat.unreadCount > 99) "99+" else if (chat.unreadCount > 0) "${chat.unreadCount}" else "",
                        color = if (chat.isMuted) palette.unreadBadgeMutedContent else palette.unreadBadgeContent,
                        fontSize = 10.sp,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }

            DropdownMenu(
                expanded = menuExpanded,
                onDismissRequest = onDismissMenu,
            ) {
                DropdownMenuItem(text = { Text(strings.unpinAction) }, onClick = onUnpin)
                DropdownMenuItem(
                    text = { Text(if (chat.isMuted) strings.unmuteAction else strings.muteAction) },
                    onClick = onMute,
                )
                DropdownMenuItem(text = { Text(strings.archiveAction) }, onClick = onArchive)
                DropdownMenuItem(
                    text = { Text(strings.delete, color = palette.destructiveRed) },
                    onClick = onDelete,
                )
            }
        }
        Spacer(Modifier.height(6.dp))
        Text(
            text = chat.title,
            color = palette.primaryText,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
private fun SwipeableAppleChatRow(
    chat: TelegramChatItem,
    densityPxThreshold: Float,
    isEditMode: Boolean,
    isSelected: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    onPin: () -> Unit,
    onArchive: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    var rawOffset by remember(chat.id) { mutableFloatStateOf(0f) }
    val animatedOffset by animateFloatAsState(targetValue = rawOffset, label = "swipeRowOffset")
    val clampMax = densityPxThreshold * 1.45f

    Box(modifier = Modifier.fillMaxWidth()) {
        if (animatedOffset > 4f) {
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .background(palette.primaryAccent)
                    .padding(start = 20.dp),
                contentAlignment = Alignment.CenterStart,
            ) {
                Text(
                    text = if (chat.isPinned) strings.unpinAction else strings.pinAction,
                    color = Color.White,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
        } else if (animatedOffset < -4f) {
            Box(
                modifier = Modifier
                    .matchParentSize()
                    .background(Color(0xFF8E8E93))
                    .padding(end = 20.dp),
                contentAlignment = Alignment.CenterEnd,
            ) {
                Text(
                    text = if (chat.isArchived) strings.unarchiveAction else strings.archiveAction,
                    color = Color.White,
                    fontSize = 13.sp,
                    fontWeight = FontWeight.SemiBold,
                )
            }
        }

        Box(
            modifier = Modifier
                .offset { IntOffset(animatedOffset.roundToInt(), 0) }
                .background(if (isSelected) palette.hoverBg else palette.appBackground)
                .pointerInput(chat.id, isEditMode) {
                    if (isEditMode) return@pointerInput
                    detectHorizontalDragGestures(
                        onDragEnd = {
                            if (rawOffset >= densityPxThreshold) onPin()
                            else if (rawOffset <= -densityPxThreshold) onArchive()
                            rawOffset = 0f
                        },
                        onDragCancel = { rawOffset = 0f },
                        onHorizontalDrag = { _, dragAmount ->
                            rawOffset = (rawOffset + dragAmount).coerceIn(-clampMax, clampMax)
                        },
                    )
                },
        ) {
            AppleChatListRow(
                chat = chat,
                isEditMode = isEditMode,
                isSelected = isSelected,
                onClick = onClick,
                onLongClick = onLongClick,
            )
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun AppleChatListRow(
    chat: TelegramChatItem,
    isEditMode: Boolean,
    isSelected: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    val liveBotStatus = formatBotActivitySubtitle(
        chat.botExecutionState,
        chat.botActivityDetail,
        if (strings.isEnglish) "en-US" else "zh-CN",
    )

    Row(
        modifier = Modifier
            .fillMaxWidth()
            .height(68.dp)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (isEditMode) {
            TelegramIcon(
                glyph = if (isSelected) TelegramGlyph.CHECK_CIRCLE else TelegramGlyph.CIRCLE_OUTLINE,
                tint = if (isSelected) palette.primaryAccent else palette.hintText,
                size = 22.dp,
            )
            Spacer(Modifier.width(10.dp))
        } else {
            // Unread blue dot (iOS Messages signature indicator)
            Box(modifier = Modifier.width(12.dp), contentAlignment = Alignment.CenterStart) {
                if (chat.unreadCount > 0 || chat.markedUnread) {
                    Box(
                        modifier = Modifier
                            .size(9.dp)
                            .clip(CircleShape)
                            .background(if (chat.isMuted) palette.unreadBadgeMuted else palette.unreadBadgeActive),
                    )
                }
            }
        }

        TelegramAvatar(
            idSeed = chat.id,
            title = chat.title,
            size = 50.dp,
            kind = chat.kind,
            online = chat.online,
            isSecret = chat.isSecretChat,
            isCompanionGhost = chat.isCompanionGhost && !chat.isBot,
            avatarShape = chat.avatarShape,
            avatarColor = chat.avatarColor,
            isBot = chat.isBot,
            botActivityActive = liveBotStatus != null,
        )

        Spacer(Modifier.width(12.dp))

        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = chat.title,
                    color = palette.primaryText,
                    fontSize = 16.sp,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                if (chat.isMuted) {
                    Spacer(Modifier.width(4.dp))
                    TelegramIcon(TelegramGlyph.BELL_SLASH, tint = palette.hintText, size = 13.dp)
                }
                Spacer(Modifier.width(6.dp))
                Text(
                    text = chat.time,
                    color = palette.secondaryText,
                    fontSize = 13.sp,
                    maxLines = 1,
                )
                Spacer(Modifier.width(3.dp))
                TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 12.dp)
            }

            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                val previewSpec = resolveConversationPreview(chat, strings)
                val previewText = if (previewSpec.prefix != null) {
                    previewSpec.prefix + previewSpec.body
                } else {
                    previewSpec.body
                }
                val previewColor = when (previewSpec.tier) {
                    ConversationPreviewTier.PENDING_ACTION -> palette.goldMerit
                    ConversationPreviewTier.DRAFT -> palette.destructiveRed
                    ConversationPreviewTier.LIVE_ACTIVITY -> palette.primaryAccent
                    ConversationPreviewTier.MESSAGE -> palette.secondaryText
                }
                Text(
                    text = previewText,
                    color = previewColor,
                    fontSize = 14.sp,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                    lineHeight = 18.sp,
                    modifier = Modifier.weight(1f),
                )
                if (chat.unreadCount > 0) {
                    Spacer(Modifier.width(8.dp))
                    Box(
                        modifier = Modifier
                            .clip(RoundedCornerShape(10.dp))
                            .background(if (chat.isMuted) palette.unreadBadgeMuted else palette.unreadBadgeActive)
                            .padding(horizontal = 6.dp, vertical = 2.dp),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            text = if (chat.unreadCount > 99) "99+" else "${chat.unreadCount}",
                            color = if (chat.isMuted) palette.unreadBadgeMutedContent else palette.unreadBadgeContent,
                            fontSize = 11.sp,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun ComposeSheetRow(
    glyph: TelegramGlyph,
    badgeColor: Color,
    title: String,
    onClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(34.dp)
                .clip(CircleShape)
                .background(badgeColor),
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(glyph = glyph, tint = Color.White, size = 17.dp)
        }
        Spacer(Modifier.width(12.dp))
        Text(
            text = title,
            color = palette.primaryText,
            fontSize = 15.sp,
            fontWeight = FontWeight.Medium,
            modifier = Modifier.weight(1f),
        )
        TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 14.dp)
    }
}

package com.ombhrum.fabushi

import android.Manifest
import android.annotation.SuppressLint
import android.content.Context
import android.content.pm.PackageManager
import android.location.LocationManager
import android.os.Build
import android.os.CancellationSignal
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Scaffold
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
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.delay


@Composable
internal fun ConversationHome(
    updateState: AndroidUpdateUiState,
    onCheckUpdate: () -> Unit,
    onInstallUpdate: () -> Unit,
    onOpenMarketplace: () -> Unit,
    onOpenRemoteComputer: () -> Unit,
    showAddMenu: Boolean,
    onShowAddMenuChange: (Boolean) -> Unit,
    showComposeMenu: Boolean,
    onShowComposeMenuChange: (Boolean) -> Unit,
    showSearch: Boolean,
    searchQuery: String,
    onShowSearchChange: (Boolean) -> Unit,
    onSearchQueryChange: (String) -> Unit,
    messagingState: MessagingUiState,
    messagingActorId: String,
    onMessagingRefresh: () -> Unit,
    onCreateDirect: (MessagingContact) -> Unit,
    onCreateConversation: (ConversationKind, String, String, List<String>) -> Unit,
    onSendText: (String, String, String?, Boolean, Long?) -> Unit,
    onSendAttachment: (String, String, String, ByteArray) -> Unit,
    onSendVoice: (String, String, String, ByteArray, List<Int>) -> Unit,
    onLoadBlob: (String, Int, (Result<ByteArray>) -> Unit) -> Unit,
    onSendContact: (String, MessagingContact) -> Unit,
    onSendPoll: (String, String, List<String>, Boolean) -> Unit,
    onVotePoll: (String, String, List<String>) -> Unit,
    onSendLocation: (String, Double, Double) -> Unit,
    onEditText: (String, String, String) -> Unit,
    onDeleteMessage: (String, String) -> Unit,
    onSetMessagePinned: (String, String, Boolean) -> Unit,
    onSetReaction: (String, String, String, Boolean) -> Unit,
    onForwardMessage: (String, String, String) -> Unit,
    onStartTyping: (String) -> Unit,
    onStopTyping: (String) -> Unit,
    onSetPinned: (ConversationSummary, Boolean) -> Unit,
    onSetArchived: (ConversationSummary, Boolean) -> Unit,
    onSetMuted: (ConversationSummary, Boolean) -> Unit,
    onMarkRead: (ConversationSummary) -> Unit,
    onSetMarkedUnread: (ConversationSummary, Boolean) -> Unit,
    onSetDraft: (String, String, String?) -> Unit,
    onUpdateConversationInfo: (String, String, String) -> Unit,
    onSetConversationParticipant: (ConversationSummary, String, String) -> Unit,
    onRemoveConversationParticipant: (String, String) -> Unit,
    onUpsertFolder: (MessagingFolder) -> Unit,
    onDeleteFolder: (String) -> Unit,
    onOpenAgentChat: () -> Unit,
    onLogout: () -> Unit,
    onSemanticContextChanged: (AndroidMobileSection?, Boolean) -> Unit = { _, _ -> },
) {
    var showContactPicker by remember { mutableStateOf(false) }
    var pendingKind by remember { mutableStateOf<ConversationKind?>(null) }
    var composeName by remember { mutableStateOf("") }
    var composeDescription by remember { mutableStateOf("") }
    var composeParticipantIds by remember { mutableStateOf(setOf<String>()) }
    var selectedConversation by remember { mutableStateOf<ConversationSummary?>(null) }
    var contextConversation by remember { mutableStateOf<ConversationSummary?>(null) }
    var activeSection by remember { mutableStateOf<AndroidMobileSection?>(null) }

    LaunchedEffect(activeSection, selectedConversation) {
        onSemanticContextChanged(activeSection, selectedConversation != null)
    }

    val conversations = messagingState.conversations
    val filteredConversations = conversations
        .filter { !it.isArchived }
        .sortedWith(compareByDescending<ConversationSummary> { it.isPinned }.thenByDescending { it.time })
        .filter { conversation -> searchQuery.isBlank() || conversation.title.contains(searchQuery, true) || conversation.preview.contains(searchQuery, true) }

    contextConversation?.let { conversation ->
        AlertDialog(
            onDismissRequest = { contextConversation = null },
            title = { Text(conversation.title) },
            text = {
                Column {
                    TextButton(onClick = { onSetPinned(conversation, !conversation.isPinned); contextConversation = null }) { Text(if (conversation.isPinned) "取消置顶" else "置顶") }
                    TextButton(onClick = { onSetMuted(conversation, !conversation.isMuted); contextConversation = null }) { Text(if (conversation.isMuted) "取消静音" else "静音") }
                    TextButton(onClick = { onSetMarkedUnread(conversation, !conversation.markedUnread); contextConversation = null }) { Text(if (conversation.markedUnread) "取消标为未读" else "标为未读") }
                    TextButton(onClick = { onSetArchived(conversation, !conversation.isArchived); contextConversation = null }) { Text(if (conversation.isArchived) "恢复" else "归档") }
                }
            },
            confirmButton = { OutlinedButton(onClick = { contextConversation = null }) { Text("取消") } },
        )
    }

    activeSection?.let { section ->
        AndroidSectionSurface(
            section = section,
            messagingState = messagingState,
            onBack = { activeSection = null },
            onCreateDirect = { contact -> activeSection = null; onCreateDirect(contact) },
            onOpenConversation = { conversation -> activeSection = null; selectedConversation = conversation; onMarkRead(conversation) },
            onUnarchive = { conversation -> onSetArchived(conversation, false) },
            onUpsertFolder = onUpsertFolder,
            onDeleteFolder = onDeleteFolder,
        )
        return
    }

    selectedConversation?.let { selected ->
        val conversation = conversations.firstOrNull { it.id == selected.id } ?: selected
        ConversationDetail(
            conversation = conversation,
            messages = messagingState.messagesByConversation[conversation.id].orEmpty(),
            sharedDraft = messagingState.draftsByConversation[conversation.id],
            onDraftChanged = { text, replyTo -> onSetDraft(conversation.id, text, replyTo) },
            currentActorId = messagingActorId,
            contacts = messagingState.contacts,
            onUpdateConversationInfo = { title, description -> onUpdateConversationInfo(conversation.id, title, description) },
            onSetConversationParticipant = { actorId, role -> onSetConversationParticipant(conversation, actorId, role) },
            onRemoveConversationParticipant = { actorId -> onRemoveConversationParticipant(conversation.id, actorId) },
            onBack = { selectedConversation = null },
            onSend = { text, replyTo, silent, scheduledAt -> onSendText(conversation.id, text, replyTo, silent, scheduledAt) },
            onSendAttachment = { fileName, mimeType, bytes -> onSendAttachment(conversation.id, fileName, mimeType, bytes) },
            onSendVoice = { fileName, mimeType, bytes, waveform -> onSendVoice(conversation.id, fileName, mimeType, bytes, waveform) },
            onLoadBlob = onLoadBlob,
            shareContacts = messagingState.contacts,
            onSendContact = { contact -> onSendContact(conversation.id, contact) },
            onSendPoll = { question, options, multiple -> onSendPoll(conversation.id, question, options, multiple) },
            onVotePoll = { messageId, optionIds -> onVotePoll(conversation.id, messageId, optionIds) },
            onSendLocation = { latitude, longitude -> onSendLocation(conversation.id, latitude, longitude) },
            onEdit = { messageId, text -> onEditText(conversation.id, messageId, text) },
            onDelete = { messageId -> onDeleteMessage(conversation.id, messageId) },
            onSetMessagePinned = { messageId, pinned -> onSetMessagePinned(conversation.id, messageId, pinned) },
            onReact = { messageId, reaction -> onSetReaction(conversation.id, messageId, reaction, true) },
            onForward = { messageId, destinationId -> onForwardMessage(conversation.id, messageId, destinationId) },
            forwardDestinations = conversations.filter { it.id != conversation.id && !it.isArchived },
            typingActorName = messagingState.typingActorByConversation[conversation.id],
            onTypingChanged = { typing -> if (typing) onStartTyping(conversation.id) else onStopTyping(conversation.id) },
            onToggleMute = { onSetMuted(conversation, !conversation.isMuted) },
            onTogglePin = { onSetPinned(conversation, !conversation.isPinned) },
            onArchive = { onSetArchived(conversation, true); selectedConversation = null },
        )
        return
    }

    if (pendingKind != null) {
        AlertDialog(
            onDismissRequest = { pendingKind = null; composeName = "" },
            title = { Text("新建${pendingKind!!.label}") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedTextField(value = composeName, onValueChange = { composeName = it }, modifier = Modifier.testTag(TestTags.ComposeName), singleLine = true, label = { Text("名称") })
                    if (pendingKind == ConversationKind.CHANNEL) {
                        OutlinedTextField(value = composeDescription, onValueChange = { composeDescription = it }, label = { Text("描述") }, maxLines = 3)
                    }
                    if (pendingKind == ConversationKind.GROUP) {
                        Text("添加成员", color = homeSecondaryText, style = MaterialTheme.typography.bodySmall)
                        messagingState.contacts.take(12).forEach { contact ->
                            val selected = contact.id in composeParticipantIds
                            Row(Modifier.fillMaxWidth().clickable { composeParticipantIds = if (selected) composeParticipantIds - contact.id else composeParticipantIds + contact.id }.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                                Text(if (selected) "●" else "○", color = if (selected) homeAccent else homeSecondaryText)
                                Text(contact.displayName, color = homePrimaryText, modifier = Modifier.padding(start = 10.dp))
                            }
                        }
                    }
                }
            },
            confirmButton = {
                Button(onClick = {
                    val title = composeName.trim()
                    if (title.isNotEmpty()) {
                        val kind = pendingKind!!
                        onCreateConversation(kind, title, composeDescription, composeParticipantIds.toList())
                        pendingKind = null
                        composeName = ""
                        composeDescription = ""
                        composeParticipantIds = emptySet()
                    }
                }, modifier = Modifier.testTag(TestTags.ComposeCreate), enabled = composeName.isNotBlank() && (pendingKind != ConversationKind.GROUP || composeParticipantIds.isNotEmpty())) { Text("创建") }
            },
            dismissButton = { OutlinedButton(onClick = { pendingKind = null; composeName = ""; composeDescription = ""; composeParticipantIds = emptySet() }) { Text("取消") } },
        )
    }

    LaunchedEffect(Unit) { onMessagingRefresh() }

    if (showContactPicker) {
        AlertDialog(
            onDismissRequest = { showContactPicker = false },
            title = { Text("新消息") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    if (messagingState.contacts.isEmpty()) Text("暂无可用联系人", color = homeSecondaryText)
                    messagingState.contacts.take(20).forEach { contact ->
                        Row(Modifier.fillMaxWidth().clickable { showContactPicker = false; onCreateDirect(contact) }.padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                            Box(Modifier.size(40.dp).background(homeAccent, CircleShape), contentAlignment = Alignment.Center) { Text(contact.displayName.take(1).uppercase(), color = Color.Black, fontWeight = FontWeight.Bold) }
                            Column(Modifier.padding(start = 12.dp)) {
                                Text(contact.displayName, color = homePrimaryText)
                                Text(contact.username?.let { "@$it" } ?: contact.kind, color = homeSecondaryText, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                    }
                }
            },
            confirmButton = { OutlinedButton(onClick = { showContactPicker = false }) { Text("取消") } },
        )
    }

    Scaffold(
        modifier = Modifier.fillMaxSize().testTag(TestTags.AppShell),
        containerColor = homeBackground,
        floatingActionButton = {
            if (!showSearch) Box {
                FloatingActionButton(
                    onClick = { onShowComposeMenuChange(true) },
                    modifier = Modifier.testTag(TestTags.AddButton),
                    containerColor = homeAccent,
                    contentColor = Color.Black,
                ) { PlusGlyph() }
                DropdownMenu(expanded = showComposeMenu, onDismissRequest = { onShowComposeMenuChange(false) }, containerColor = homeSurface) {
                    DropdownMenuItem(text = { Text("新消息", color = homePrimaryText) }, onClick = { onShowComposeMenuChange(false); showContactPicker = true })
                    DropdownMenuItem(text = { Text("新建群组", color = homePrimaryText) }, onClick = { onShowComposeMenuChange(false); pendingKind = ConversationKind.GROUP })
                    DropdownMenuItem(text = { Text("新建频道", color = homePrimaryText) }, onClick = { onShowComposeMenuChange(false); pendingKind = ConversationKind.CHANNEL })
                    DropdownMenuItem(text = { Text("联系人分组", color = homePrimaryText) }, onClick = { onShowComposeMenuChange(false) })
                }
            }
        },
    ) { padding ->
        LazyColumn(Modifier.fillMaxSize().padding(padding).testTag(TestTags.ConversationList)) {
            item {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
                    horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box {
                        ProfileAvatar(onClick = { onShowAddMenuChange(true) })
                        DropdownMenu(expanded = showAddMenu, onDismissRequest = { onShowAddMenuChange(false) }, containerColor = homeSurface) {
                            DropdownMenuItem(text = { Text("聊天", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false) })
                            DropdownMenuItem(text = { Text("联系人", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.CONTACTS })
                            DropdownMenuItem(text = { Text("Bots", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.BOTS })
                            DropdownMenuItem(text = { Text("群组", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.GROUPS })
                            DropdownMenuItem(text = { Text("频道", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.CHANNELS })
                            DropdownMenuItem(text = { Text("通话", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.CALLS })
                            DropdownMenuItem(text = { Text("收藏", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.SAVED })
                            DropdownMenuItem(text = { Text("归档", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.ARCHIVE })
                            DropdownMenuItem(text = { Text("文件夹", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.FOLDERS })
                            DropdownMenuItem(text = { Text("设置", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); activeSection = AndroidMobileSection.SETTINGS })
                            DropdownMenuItem(modifier = Modifier.testTag(TestTags.MarketplaceEntry), text = { Text("Mini Apps / 插件市场", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); onOpenMarketplace() })
                            DropdownMenuItem(modifier = Modifier.testTag(TestTags.RemoteComputerEntry), text = { Text("我的电脑", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); onOpenRemoteComputer() })
                            DropdownMenuItem(modifier = Modifier.testTag(TestTags.MobileLogout), text = { Text("退出登录", color = Color(0xFFFF6B6B)) }, onClick = { onShowAddMenuChange(false); onLogout() })
                            if (updateState.phase != AndroidUpdatePhase.DISABLED) DropdownMenuItem(text = { Text("检查更新", color = homePrimaryText) }, onClick = { onShowAddMenuChange(false); onCheckUpdate() })
                        }
                    }
                    Text("聊天", color = homePrimaryText, fontWeight = FontWeight.SemiBold, fontSize = 20.sp)
                    CircularActionButton(TestTags.HomeSearchButton, if (showSearch) "关闭搜索" else "搜索对话", { onShowSearchChange(!showSearch) }) { SearchGlyph() }
                }
            }
            if (showSearch) item {
                OutlinedTextField(
                    value = searchQuery, onValueChange = onSearchQueryChange,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp).testTag(TestTags.HomeSearchField),
                    singleLine = true, placeholder = { Text("搜索", color = homeSecondaryText) },
                    colors = OutlinedTextFieldDefaults.colors(focusedTextColor = homePrimaryText, unfocusedTextColor = homePrimaryText, focusedContainerColor = homeSurface, unfocusedContainerColor = homeSurface),
                    shape = RoundedCornerShape(14.dp),
                )
            }
            if (shouldShowUpdateBanner(updateState.phase)) item { UpdateBanner(updateState, onCheckUpdate, onInstallUpdate) }
            if (conversations.any { it.isArchived } && searchQuery.isBlank()) item {
                Row(Modifier.fillMaxWidth().clickable { activeSection = AndroidMobileSection.ARCHIVE }.padding(horizontal = 18.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text("▣", color = homeSecondaryText); Text("已归档", color = homePrimaryText, modifier = Modifier.padding(start = 12.dp).weight(1f)); Text("${conversations.count { it.isArchived }}", color = homeSecondaryText)
                }
            }
            if (searchQuery.isBlank()) item {
                Row(
                    Modifier.fillMaxWidth().testTag(TestTags.MahayanaAgentEntry).clickable(onClick = onOpenAgentChat).padding(horizontal = 18.dp, vertical = 11.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.size(54.dp).background(homeSurface, CircleShape).border(1.dp, homeBorder, CircleShape), contentAlignment = Alignment.Center) {
                        Text("✦", color = homeAccent, fontSize = 30.sp, fontWeight = FontWeight.Bold)
                    }
                    Column(Modifier.padding(start = 13.dp).weight(1f)) {
                        Text("大乘助手", color = homePrimaryText, fontWeight = FontWeight.SemiBold, fontSize = 17.sp)
                        Text("Mahayana 多步骤智能体 · 实时工作流", color = homeSecondaryText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                    Text("›", color = homeSecondaryText, fontSize = 24.sp)
                }
            }
            if (filteredConversations.isEmpty() && searchQuery.isNotBlank()) item {
                Column(Modifier.fillMaxWidth().padding(vertical = 88.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(if (searchQuery.isBlank()) "还没有对话" else "没有找到结果", color = homeSecondaryText, fontWeight = FontWeight.SemiBold)
                    Text(if (searchQuery.isBlank()) "点击右下角写消息按钮开始聊天" else "尝试其他关键词", color = homeSecondaryText, style = MaterialTheme.typography.bodySmall)
                }
            } else items(filteredConversations, key = { it.id }) { conversation ->
                ConversationRow(conversation, onClick = {
                    selectedConversation = conversation
                    onMarkRead(conversation)
                }, onLongClick = { contextConversation = conversation })
            }
            item { Spacer(Modifier.height(88.dp)) }
        }
    }
}


@Composable
internal fun ConversationDetail(
    conversation: ConversationSummary,
    messages: List<ChatMessage>,
    sharedDraft: MessagingDraft?,
    onDraftChanged: (String, String?) -> Unit,
    currentActorId: String,
    contacts: List<MessagingContact>,
    onUpdateConversationInfo: (String, String) -> Unit,
    onSetConversationParticipant: (String, String) -> Unit,
    onRemoveConversationParticipant: (String) -> Unit,
    onBack: () -> Unit,
    onSend: (String, String?, Boolean, Long?) -> Unit,
    onSendAttachment: (String, String, ByteArray) -> Unit,
    onSendVoice: (String, String, ByteArray, List<Int>) -> Unit,
    onLoadBlob: (String, Int, (Result<ByteArray>) -> Unit) -> Unit,
    shareContacts: List<MessagingContact>,
    onSendContact: (MessagingContact) -> Unit,
    onSendPoll: (String, List<String>, Boolean) -> Unit,
    onVotePoll: (String, List<String>) -> Unit,
    onSendLocation: (Double, Double) -> Unit,
    onEdit: (String, String) -> Unit,
    onDelete: (String) -> Unit,
    onSetMessagePinned: (String, Boolean) -> Unit,
    onReact: (String, String) -> Unit,
    onForward: (String, String) -> Unit,
    forwardDestinations: List<ConversationSummary>,
    typingActorName: String?,
    onTypingChanged: (Boolean) -> Unit,
    onToggleMute: () -> Unit,
    onTogglePin: () -> Unit,
    onArchive: () -> Unit,
) {
    val draftSnapshot = remember(conversation.id, sharedDraft?.updatedAtMs) {
        coordinatorComposerDraftSnapshot(conversation.id, sharedDraft)
    }
    var draft by remember(conversation.id, draftSnapshot.updatedAtMs) {
        mutableStateOf(draftSnapshot.text)
    }
    var selectedMessage by remember { mutableStateOf<ChatMessage?>(null) }
    var replyTarget by remember(conversation.id, draftSnapshot.updatedAtMs) {
        mutableStateOf(
            draftSnapshot.replyToMessageId?.let { replyId ->
                messages.firstOrNull { it.id == replyId }
            },
        )
    }
    var editingMessage by remember { mutableStateOf<ChatMessage?>(null) }
    var forwardingMessage by remember { mutableStateOf<ChatMessage?>(null) }
    var mediaViewerMessage by remember { mutableStateOf<ChatMessage?>(null) }
    var showConversationInfo by remember { mutableStateOf(false) }
    var showChatSearch by remember { mutableStateOf(false) }
    var chatSearchQuery by remember { mutableStateOf("") }
    var chatSearchIndex by remember(conversation.id) { mutableStateOf(0) }
    val chatSearchMatches = remember(messages, chatSearchQuery) {
        findInChatMatches(messages, chatSearchQuery)
    }
    LaunchedEffect(chatSearchQuery, chatSearchMatches.size) {
        chatSearchIndex = when {
            chatSearchMatches.isEmpty() -> -1
            chatSearchIndex !in chatSearchMatches.indices -> 0
            else -> chatSearchIndex
        }
    }
    var showSendModes by remember { mutableStateOf(false) }
    var showContactShare by remember { mutableStateOf(false) }
    var showPollComposer by remember { mutableStateOf(false) }
    var pollQuestion by remember { mutableStateOf("") }
    var pollOption1 by remember { mutableStateOf("") }
    var pollOption2 by remember { mutableStateOf("") }
    var pollOption3 by remember { mutableStateOf("") }
    val context = LocalContext.current
    val voiceRecorder = remember { NativeVoiceRecorder(context) }
    val voicePlayer = remember { NativeVoicePlayer(context) }
    var playingVoiceMessageId by remember { mutableStateOf<String?>(null) }
    var isRecordingVoice by remember { mutableStateOf(false) }
    var recordingSeconds by remember { mutableStateOf(0) }
    var voiceError by remember { mutableStateOf<String?>(null) }
    DisposableEffect(Unit) { onDispose { voiceRecorder.cancel(); voicePlayer.stop() } }
    LaunchedEffect(isRecordingVoice) {
        recordingSeconds = 0
        while (isRecordingVoice) { delay(1000); if (isRecordingVoice) recordingSeconds += 1 }
    }
    LaunchedEffect(
        conversation.id,
        draft,
        replyTarget?.id,
        editingMessage?.id,
        draftSnapshot.updatedAtMs,
    ) {
        delay(350)
        if (
            editingMessage == null &&
            shouldPersistCoordinatorDraft(draftSnapshot, draft, replyTarget?.id)
        ) {
            onDraftChanged(draft, replyTarget?.id)
        }
    }
    var showLocationShare by remember { mutableStateOf(false) }
    var currentLocation by remember { mutableStateOf<Pair<Double, Double>?>(null) }
    var locationError by remember { mutableStateOf<String?>(null) }
    fun resolveLocation() {
        requestFabushiCurrentLocation(context) { latitude, longitude, error ->
            currentLocation = if (latitude != null && longitude != null) latitude to longitude else null
            locationError = error
        }
    }
    val locationPermissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { grants ->
        if (grants.values.any { it }) resolveLocation() else locationError = "请允许位置权限后再分享位置"
    }
    val microphonePermissionLauncher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        if (granted) {
            voiceRecorder.start().onSuccess { isRecordingVoice = true; voiceError = null }.onFailure { voiceError = it.message }
        } else voiceError = "请允许麦克风权限后再发送语音"
    }
    val attachmentLauncher = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) {
            runCatching {
                val bytes = context.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@runCatching
                val mime = context.contentResolver.getType(uri) ?: "application/octet-stream"
                val name = uri.lastPathSegment?.substringAfterLast('/') ?: "attachment"
                onSendAttachment(name, mime, bytes)
            }
        }
    }

    if (showConversationInfo) {
        AndroidConversationInfo(
            conversation = conversation, contacts = contacts, currentActorId = currentActorId, onBack = { showConversationInfo = false },
            onUpdateInfo = onUpdateConversationInfo, onSetParticipant = onSetConversationParticipant, onRemoveParticipant = onRemoveConversationParticipant,
        )
        return
    }

    mediaViewerMessage?.let { mediaMessage ->
        AndroidMediaViewer(message = mediaMessage, onLoadBlob = onLoadBlob, onClose = { mediaViewerMessage = null })
        return
    }

    if (showSendModes) {
        AlertDialog(
            onDismissRequest = { showSendModes = false }, title = { Text("发送方式") },
            text = {
                Column {
                    TextButton(onClick = {
                        val text = draft.trim(); if (text.isNotEmpty()) { onSend(text, replyTarget?.id, true, null); onDraftChanged("", null); draft = ""; replyTarget = null; editingMessage = null }; showSendModes = false
                    }) { Text("静默发送") }
                    TextButton(onClick = {
                        val text = draft.trim(); if (text.isNotEmpty()) { onSend(text, replyTarget?.id, false, System.currentTimeMillis() + 3_600_000); onDraftChanged("", null); draft = ""; replyTarget = null; editingMessage = null }; showSendModes = false
                    }) { Text("1 小时后发送") }
                    TextButton(onClick = {
                        val calendar = java.util.Calendar.getInstance().apply { add(java.util.Calendar.DAY_OF_YEAR, 1); set(java.util.Calendar.HOUR_OF_DAY, 9); set(java.util.Calendar.MINUTE, 0); set(java.util.Calendar.SECOND, 0); set(java.util.Calendar.MILLISECOND, 0) }
                        val text = draft.trim(); if (text.isNotEmpty()) { onSend(text, replyTarget?.id, false, calendar.timeInMillis); onDraftChanged("", null); draft = ""; replyTarget = null; editingMessage = null }; showSendModes = false
                    }) { Text("明天上午 9:00") }
                }
            },
            confirmButton = { OutlinedButton(onClick = { showSendModes = false }) { Text("取消") } },
        )
    }

    if (showLocationShare) {
        AlertDialog(
            onDismissRequest = { showLocationShare = false }, title = { Text("发送位置") },
            text = {
                Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
                    val location = currentLocation
                    if (location != null) {
                        Text("当前位置", color = homePrimaryText, fontWeight = FontWeight.SemiBold)
                        Text("纬度 %.6f".format(location.first), color = homeSecondaryText, modifier = Modifier.padding(top = 8.dp))
                        Text("经度 %.6f".format(location.second), color = homeSecondaryText)
                    } else {
                        Text(locationError ?: "正在获取位置…", color = homeSecondaryText)
                    }
                }
            },
            confirmButton = {
                val location = currentLocation
                Button(onClick = { if (location != null) { onSendLocation(location.first, location.second); showLocationShare = false } }, enabled = location != null) { Text("发送") }
            },
            dismissButton = { OutlinedButton(onClick = { showLocationShare = false }) { Text("取消") } },
        )
    }

    if (showContactShare) {
        AlertDialog(
            onDismissRequest = { showContactShare = false }, title = { Text("发送联系人") },
            text = {
                Column {
                    if (shareContacts.isEmpty()) Text("暂无可用联系人", color = homeSecondaryText)
                    shareContacts.take(20).forEach { contact -> TextButton(onClick = { onSendContact(contact); showContactShare = false }) { Text(contact.displayName) } }
                }
            },
            confirmButton = { OutlinedButton(onClick = { showContactShare = false }) { Text("取消") } },
        )
    }
    if (showPollComposer) {
        AlertDialog(
            onDismissRequest = { showPollComposer = false }, title = { Text("新建投票") },
            text = { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(pollQuestion, { pollQuestion = it }, label = { Text("问题") })
                OutlinedTextField(pollOption1, { pollOption1 = it }, label = { Text("选项 1") })
                OutlinedTextField(pollOption2, { pollOption2 = it }, label = { Text("选项 2") })
                OutlinedTextField(pollOption3, { pollOption3 = it }, label = { Text("选项 3（可选）") })
            } },
            confirmButton = { Button(onClick = { onSendPoll(pollQuestion, listOf(pollOption1, pollOption2, pollOption3), false); showPollComposer = false }, enabled = pollQuestion.isNotBlank() && pollOption1.isNotBlank() && pollOption2.isNotBlank()) { Text("发送") } },
            dismissButton = { OutlinedButton(onClick = { showPollComposer = false }) { Text("取消") } },
        )
    }

    selectedMessage?.let { message ->
        AlertDialog(
            onDismissRequest = { selectedMessage = null },
            title = { Text("消息操作") },
            text = {
                Column {
                    TextButton(onClick = { replyTarget = message; editingMessage = null; selectedMessage = null }) { Text("回复") }
                    TextButton(onClick = { forwardingMessage = message; selectedMessage = null }) { Text("转发") }
                    TextButton(onClick = { onReact(message.id, "👍"); selectedMessage = null }) { Text("👍 赞") }
                    if (message.outgoing) TextButton(onClick = { editingMessage = message; replyTarget = null; draft = message.text; selectedMessage = null }) { Text("编辑") }
                    TextButton(onClick = { onSetMessagePinned(message.id, !message.pinned); selectedMessage = null }) { Text(if (message.pinned) "取消置顶消息" else "置顶消息") }
                    TextButton(onClick = { onDelete(message.id); selectedMessage = null }) { Text("删除") }
                }
            },
            confirmButton = { OutlinedButton(onClick = { selectedMessage = null }) { Text("取消") } },
        )
    }

    forwardingMessage?.let { message ->
        AlertDialog(
            onDismissRequest = { forwardingMessage = null },
            title = { Text("转发到") },
            text = {
                Column {
                    if (forwardDestinations.isEmpty()) Text("暂无其他会话", color = homeSecondaryText)
                    forwardDestinations.take(20).forEach { destination ->
                        TextButton(onClick = { onForward(message.id, destination.id); forwardingMessage = null }) { Text(destination.title) }
                    }
                }
            },
            confirmButton = { OutlinedButton(onClick = { forwardingMessage = null }) { Text("取消") } },
        )
    }

    Scaffold(containerColor = homeBackground) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            ConversationChatHeader(
                conversation = conversation,
                searchOpen = showChatSearch,
                onBack = onBack,
                onOpenInfo = { showConversationInfo = true },
                onToggleSearch = {
                    showChatSearch = !showChatSearch
                    if (!showChatSearch) {
                        chatSearchQuery = ""
                        chatSearchIndex = -1
                    }
                },
                onToggleMute = onToggleMute,
                onTogglePin = onTogglePin,
                onArchive = onArchive,
            )
            val pinnedMessage = conversation.pinnedMessageIds.lastOrNull()?.let { pinnedId -> messages.firstOrNull { it.id == pinnedId } }
            if (pinnedMessage != null) {
                Row(Modifier.fillMaxWidth().background(homeSurface).padding(horizontal = 12.dp, vertical = 7.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(width = 3.dp, height = 34.dp).background(homeAccent))
                    Column(Modifier.weight(1f).padding(start = 9.dp)) { Text("置顶消息", color = homeAccent, style = MaterialTheme.typography.bodySmall, fontWeight = FontWeight.Bold); Text(pinnedMessage.text, color = homePrimaryText, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis) }
                    Text("×", color = homeSecondaryText, fontSize = 22.sp, modifier = Modifier.clickable { onSetMessagePinned(pinnedMessage.id, false) }.padding(6.dp))
                }
            }
            if (showChatSearch) {
                FindInChatBar(
                    query = chatSearchQuery,
                    matchCount = chatSearchMatches.size,
                    currentIndex = chatSearchIndex,
                    onQueryChange = {
                        chatSearchQuery = it
                        chatSearchIndex = 0
                    },
                    onPrevious = {
                        chatSearchIndex = stepFindInChatIndex(
                            chatSearchIndex,
                            -1,
                            chatSearchMatches.size,
                        )
                    },
                    onNext = {
                        chatSearchIndex = stepFindInChatIndex(
                            chatSearchIndex,
                            1,
                            chatSearchMatches.size,
                        )
                    },
                    onClose = {
                        showChatSearch = false
                        chatSearchQuery = ""
                        chatSearchIndex = -1
                    },
                )
            }
            if (typingActorName != null) {
                Text("$typingActorName 正在输入…", color = homeSecondaryText, style = MaterialTheme.typography.bodySmall, modifier = Modifier.fillMaxWidth().padding(horizontal = 18.dp, vertical = 4.dp))
            }
            ConversationTranscript(
                conversationTitle = conversation.title,
                messages = messages,
                searchQuery = chatSearchQuery,
                currentFindMessageId = chatSearchMatches
                    .getOrNull(chatSearchIndex)
                    ?.messageId,
                playingVoiceMessageId = playingVoiceMessageId,
                onPlayVoice = { message ->
                    val blobId = message.mediaBlobId
                    if (playingVoiceMessageId == message.id) {
                        voicePlayer.stop()
                        playingVoiceMessageId = null
                    } else if (blobId != null && message.mediaSizeBytes > 0) {
                        onLoadBlob(blobId, message.mediaSizeBytes) { result ->
                            result.onSuccess { bytes ->
                                voicePlayer.toggle(message.id, bytes) {
                                    playingVoiceMessageId = null
                                }.onSuccess { playing ->
                                    playingVoiceMessageId = if (playing) message.id else null
                                }
                            }
                        }
                    }
                },
                onOpenMedia = { mediaViewerMessage = it },
                onVotePoll = onVotePoll,
                onSelectMessage = { selectedMessage = it },
                onReply = {
                    replyTarget = it
                    editingMessage = null
                },
                modifier = Modifier.weight(1f),
            )
            ConversationComposer(
                draft = draft,
                editingMessage = editingMessage,
                replyTarget = replyTarget,
                isRecordingVoice = isRecordingVoice,
                recordingSeconds = recordingSeconds,
                voiceError = voiceError,
                onDraftChange = { draft = it },
                onTypingChanged = onTypingChanged,
                onClearContext = {
                    editingMessage = null
                    replyTarget = null
                },
                onPickAttachment = { mime ->
                    attachmentLauncher.launch(mime)
                },
                onRequestLocation = {
                    showLocationShare = true
                    currentLocation = null
                    locationError = null
                    val fine = ContextCompat.checkSelfPermission(
                        context,
                        Manifest.permission.ACCESS_FINE_LOCATION,
                    ) == PackageManager.PERMISSION_GRANTED
                    val coarse = ContextCompat.checkSelfPermission(
                        context,
                        Manifest.permission.ACCESS_COARSE_LOCATION,
                    ) == PackageManager.PERMISSION_GRANTED
                    if (fine || coarse) {
                        resolveLocation()
                    } else {
                        locationPermissionLauncher.launch(
                            arrayOf(
                                Manifest.permission.ACCESS_FINE_LOCATION,
                                Manifest.permission.ACCESS_COARSE_LOCATION,
                            ),
                        )
                    }
                },
                onRequestContact = { showContactShare = true },
                onRequestPoll = {
                    pollQuestion = ""
                    pollOption1 = ""
                    pollOption2 = ""
                    pollOption3 = ""
                    showPollComposer = true
                },
                onCancelRecording = {
                    voiceRecorder.cancel()
                    isRecordingVoice = false
                },
                onFinishRecording = {
                    voiceRecorder.stop()
                        .onSuccess { recording ->
                            onSendVoice(
                                recording.file.name,
                                "audio/mp4",
                                recording.bytes,
                                emptyList(),
                            )
                            isRecordingVoice = false
                            voiceError = null
                        }
                        .onFailure {
                            voiceError = it.message
                            isRecordingVoice = false
                        }
                },
                onStartRecording = {
                    val granted = ContextCompat.checkSelfPermission(
                        context,
                        Manifest.permission.RECORD_AUDIO,
                    ) == PackageManager.PERMISSION_GRANTED
                    if (granted) {
                        voiceRecorder.start()
                            .onSuccess {
                                isRecordingVoice = true
                                voiceError = null
                            }
                            .onFailure { voiceError = it.message }
                    } else {
                        microphonePermissionLauncher.launch(Manifest.permission.RECORD_AUDIO)
                    }
                },
                onEdit = onEdit,
                onSend = { text, replyTo ->
                    onSend(text, replyTo, false, null)
                    onDraftChanged("", null)
                },
                onRequestSendModes = { showSendModes = true },
            )
        }
    }
}

@SuppressLint("MissingPermission")
internal fun requestFabushiCurrentLocation(context: Context, callback: (Double?, Double?, String?) -> Unit) {
    val manager = context.getSystemService(Context.LOCATION_SERVICE) as? LocationManager
        ?: return callback(null, null, "设备不支持位置服务")
    val provider = when {
        manager.isProviderEnabled(LocationManager.GPS_PROVIDER) -> LocationManager.GPS_PROVIDER
        manager.isProviderEnabled(LocationManager.NETWORK_PROVIDER) -> LocationManager.NETWORK_PROVIDER
        else -> return callback(null, null, "请先开启系统位置服务")
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
        manager.getCurrentLocation(provider, CancellationSignal(), context.mainExecutor) { location ->
            if (location == null) callback(null, null, "暂时无法获取当前位置") else callback(location.latitude, location.longitude, null)
        }
    } else {
        val location = manager.getLastKnownLocation(provider)
        if (location == null) callback(null, null, "暂时无法获取当前位置") else callback(location.latitude, location.longitude, null)
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun ConversationRow(conversation: ConversationSummary, onClick: () -> Unit, onLongClick: () -> Unit = {}) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(start = 30.dp, end = 24.dp, top = 13.dp, bottom = 13.dp)
            ,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier.size(54.dp).background(conversationAccent, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(conversation.badge, color = Color(0xFF1A1009), fontWeight = FontWeight.Black, fontSize = 13.sp)
        }
        Column(
            modifier = Modifier.weight(1f).padding(start = 18.dp, end = 12.dp),
            verticalArrangement = Arrangement.spacedBy(5.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    conversation.title, color = homePrimaryText, style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
                if (conversation.isMuted) Text("⌁", color = homeSecondaryText, fontSize = 12.sp)
                if (conversation.isPinned) Text("⌖", color = homeSecondaryText, fontSize = 12.sp)
            }
            Text(
                conversation.preview,
                color = homeSecondaryText,
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(5.dp)) {
            Text(conversation.time, color = Color(0xFF56565B), style = MaterialTheme.typography.bodySmall)
            if (conversation.unreadCount > 0) {
                Text("${conversation.unreadCount}", color = Color.Black, fontWeight = FontWeight.Bold, fontSize = 11.sp, modifier = Modifier.background(homeAccent, CircleShape).padding(horizontal = 6.dp, vertical = 2.dp))
            }
        }
    }
}


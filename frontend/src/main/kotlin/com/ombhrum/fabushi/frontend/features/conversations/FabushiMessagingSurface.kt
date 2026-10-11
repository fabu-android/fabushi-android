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

object TestTags {
    const val AppShell = "app-shell"
    const val Home = "home"
    const val ProfileAvatar = "profile-avatar"
    const val HomeSearchButton = "home-search-button"
    const val HomeSearchField = "home-search-field"
    const val AddButton = "home-add-button"
    const val ConversationList = "conversation-list"
    const val ConversationRow = "conversation-chief-of-staff"
    const val ComposeName = "compose-name"
    const val ComposeCreate = "compose-create"
    const val MarketplaceEntry = "marketplace-entry"
    const val MarketplaceBack = "marketplace-back"
    const val RemoteComputerEntry = "remote-computer-entry"
    const val RemoteComputerSurface = "remote-computer-surface"
    const val RemoteComputerClose = "remote-computer-close"
    const val RemoteComputerStatus = "remote-computer-status"
    const val RemoteComputerLoading = "remote-computer-loading"
    const val RemoteComputerError = "remote-computer-error"
    const val RemoteComputerReload = "remote-computer-reload"
    const val RemoteComputerWebView = "remote-computer-webview"
    const val RuntimeBadge = "runtime-badge"
    const val SearchField = "marketplace-search"
    const val SearchButton = "marketplace-search-submit"
    const val HostStatus = "host-status"
    const val MobileOnboarding = "mobile-onboarding"
    const val MobileOnboardingContinue = "mobile-onboarding-continue"
    const val MobileOnboardingSkip = "mobile-onboarding-skip"
    const val MobileLogin = "mobile-login"
    const val MobileLoginBrowser = "mobile-login-browser"
    const val MobileLoginReopen = "mobile-login-reopen"
    const val MobileLoginCancel = "mobile-login-cancel"
    const val MobileLogout = "mobile-logout"
    const val MahayanaAgentEntry = "mahayana-agent-entry"
    const val MahayanaAgentChat = "mahayana-agent-chat"
    const val MahayanaSend = "mahayana-send"
    const val MahayanaStop = "mahayana-stop"
    const val MahayanaThinking = "mahayana-thinking"
    const val MahayanaStep = "mahayana-step"
    const val PermissionDialog = "permission-dialog"
    const val PermissionApprove = "permission-approve"
    const val PermissionDeny = "permission-deny"
    const val UpdateCard = "android-update-card"
    const val UpdateAction = "android-update-action"
    fun plugin(id: String) = "plugin-$id"
    fun install(id: String) = "install-$id"
    fun open(id: String) = "open-$id"
}

private enum class MobileDestination { HOME, MARKETPLACE, REMOTE_COMPUTER }
internal enum class AndroidMobileSection(val label: String) { CONTACTS("联系人"), BOTS("Bots"), GROUPS("群组"), CHANNELS("频道"), SAVED("收藏"), ARCHIVE("归档"), CALLS("通话"), FOLDERS("文件夹"), SETTINGS("设置") }

internal object MahayanaAssistantSemanticProjection {
    const val AgentId = "test:peer-legacy:conversation:mahayana-ai:agent:assistant"
    const val UnreadAgentId = "peer-unread:legacy:conversation:mahayana-ai:agent:assistant"
    const val UnreadNoneName = "unread-none"
    const val UnreadPositiveName = "unread-positive"

    fun unreadName(hasUnread: Boolean): String =
        if (hasUnread) UnreadPositiveName else UnreadNoneName

    fun visible(
        destinationIsHome: Boolean,
        activeSection: AndroidMobileSection?,
        regularConversationOpen: Boolean,
        assistantOpen: Boolean,
    ): Boolean =
        destinationIsHome &&
            (assistantOpen || (activeSection == null && !regularConversationOpen))

    fun unreadVisible(
        destinationIsHome: Boolean,
        activeSection: AndroidMobileSection?,
        regularConversationOpen: Boolean,
        assistantOpen: Boolean,
    ): Boolean =
        visible(destinationIsHome, activeSection, regularConversationOpen, assistantOpen) &&
            !assistantOpen
}
internal val homeBackground = Color(0xFF0B0B0C)
internal val homeSurface = Color(0xFF151516)
internal val homeBorder = Color(0xFF29292B)
internal val homePrimaryText = Color(0xFFF3F3F4)
internal val homeSecondaryText = Color(0xFF8C8C91)
internal val homeAccent = Color(0xFFFFB21A)
internal val conversationAccent = Color(0xFFFF5A0A)

@Composable
fun FabushiMessagingSurface(
    state: MarketplaceUiState,
    onQueryChange: (String) -> Unit,
    onSearch: () -> Unit,
    onInstall: (MarketplacePlugin) -> Unit,
    onOpen: (MarketplacePlugin) -> Unit,
    onApprovePermissions: () -> Unit,
    onDenyPermissions: () -> Unit,
    onSubmitPluginVariables: (Map<String, String>) -> Unit,
    onCancelPluginVariables: () -> Unit,
    updateState: AndroidUpdateUiState = AndroidUpdateUiState(
        phase = AndroidUpdatePhase.DISABLED,
        currentVersion = BuildConfig.VERSION_NAME,
    ),
    onCheckUpdate: () -> Unit = {},
    onInstallUpdate: () -> Unit = {},
    messagingState: MessagingUiState = MessagingUiState(),
    messagingActorId: String = "",
    onMessagingRefresh: () -> Unit = {},
    onCreateDirect: (MessagingContact) -> Unit = {},
    onCreateConversation: (ConversationKind, String, String, List<String>) -> Unit = { _, _, _, _ -> },
    onSendText: (String, String, String?, Boolean, Long?) -> Unit = { _, _, _, _, _ -> },
    onSendAttachment: (String, String, String, ByteArray) -> Unit = { _, _, _, _ -> },
    onSendVoice: (String, String, String, ByteArray, List<Int>) -> Unit = { _, _, _, _, _ -> },
    onLoadBlob: (String, Int, (Result<ByteArray>) -> Unit) -> Unit = { _, _, callback -> callback(Result.failure(IllegalStateException("Blob loader unavailable"))) },
    onSendContact: (String, MessagingContact) -> Unit = { _, _ -> },
    onSendPoll: (String, String, List<String>, Boolean) -> Unit = { _, _, _, _ -> },
    onVotePoll: (String, String, List<String>) -> Unit = { _, _, _ -> },
    onSendLocation: (String, Double, Double) -> Unit = { _, _, _ -> },
    onEditText: (String, String, String) -> Unit = { _, _, _ -> },
    onDeleteMessage: (String, String) -> Unit = { _, _ -> },
    onSetMessagePinned: (String, String, Boolean) -> Unit = { _, _, _ -> },
    onSetReaction: (String, String, String, Boolean) -> Unit = { _, _, _, _ -> },
    onForwardMessage: (String, String, List<String>) -> Unit = { _, _, _ -> },
    onStartTyping: (String) -> Unit = {},
    onStopTyping: (String) -> Unit = {},
    onSetPinned: (ConversationSummary, Boolean) -> Unit = { _, _ -> },
    onSetArchived: (ConversationSummary, Boolean) -> Unit = { _, _ -> },
    onSetMuted: (ConversationSummary, Boolean) -> Unit = { _, _ -> },
    onMarkRead: (ConversationSummary) -> Unit = {},
    onSetMarkedUnread: (ConversationSummary, Boolean) -> Unit = { _, _ -> },
    onSetDraft: (String, String, String?) -> Unit = { _, _, _ -> },
    onUpdateConversationInfo: (String, String, String) -> Unit = { _, _, _ -> },
    onSetConversationParticipant: (ConversationSummary, String, String) -> Unit = { _, _, _ -> },
    onRemoveConversationParticipant: (String, String) -> Unit = { _, _ -> },
    onUpsertFolder: (MessagingFolder) -> Unit = {},
    onDeleteFolder: (String) -> Unit = {},
    appAgentSurface: FabushiAppAgentSurface? = null,
    authGateEnabled: Boolean = false,
    onAdvanceOnboarding: () -> Unit = {},
    onSkipOnboarding: () -> Unit = {},
    onBeginBrowserLogin: () -> Unit = {},
    onReopenBrowserLogin: () -> Unit = {},
    onCancelBrowserLogin: () -> Unit = {},
    onLogout: () -> Unit = {},
    onBackToGrokHome: () -> Unit = {},
    onChatDraftChange: (String) -> Unit = {},
    onSendChat: () -> Unit = {},
    onStopChat: () -> Unit = {},
    onMarkAssistantRead: () -> Unit = {},
    onOpenGeneratedMiniApp: (MobileChatMessage) -> Unit = {},
) {
    var destination by remember { mutableStateOf(MobileDestination.HOME) }
    var showAddMenu by remember { mutableStateOf(false) }
    var showAgentChat by remember { mutableStateOf(false) }
    var showHomeSearch by remember { mutableStateOf(false) }
    var homeSearchQuery by remember { mutableStateOf("") }
    var showComposeMenu by remember { mutableStateOf(false) }
    var semanticSection by remember { mutableStateOf<AndroidMobileSection?>(null) }
    var semanticConversationOpen by remember { mutableStateOf(false) }
    val latestAssistantMessageId = state.chatMessages
        .lastOrNull { it.kind == MobileChatEntryKind.MESSAGE && it.role == MobileChatRole.ASSISTANT }
        ?.id

    LaunchedEffect(showAgentChat, latestAssistantMessageId) {
        if (showAgentChat) onMarkAssistantRead()
    }

    if (authGateEnabled && state.onboardingStep < 3) {
        MobileOnboarding(state.onboardingStep, onAdvanceOnboarding, onSkipOnboarding)
        return
    }
    if (authGateEnabled && !state.authResolved) {
        MobileAuthLoading()
        return
    }
    if (authGateEnabled && !state.loggedIn) {
        MobileLogin(state, onBeginBrowserLogin, onReopenBrowserLogin, onCancelBrowserLogin)
        return
    }
    LaunchedEffect(
        destination,
        showAgentChat,
        semanticSection,
        semanticConversationOpen,
        showAddMenu,
        showHomeSearch,
        homeSearchQuery,
        showComposeMenu,
        state,
        updateState.phase,
        appAgentSurface,
    ) {
        val elements = mutableListOf<FabushiAppAgentSurface.Element>()
        val actions = linkedMapOf<String, FabushiAppAgentSurface.Action>()
        fun element(
            id: String,
            role: String,
            name: String,
            enabled: Boolean = true,
            visible: Boolean = true,
            action: FabushiAppAgentSurface.Action? = null,
        ) {
            val normalizedId = id
                .replace(Regex("[^A-Za-z0-9._:/@-]"), "-")
                .take(200)
            elements += FabushiAppAgentSurface.Element(
                agentId = normalizedId,
                role = role.take(80),
                name = name.take(240),
                visible = visible,
                enabled = enabled,
            )
            if (action != null) actions[normalizedId] = action
        }
        val assistantVisible = MahayanaAssistantSemanticProjection.visible(
            destinationIsHome = destination == MobileDestination.HOME,
            activeSection = semanticSection,
            regularConversationOpen = semanticConversationOpen,
            assistantOpen = showAgentChat,
        )
        val assistantUnreadVisible = MahayanaAssistantSemanticProjection.unreadVisible(
            destinationIsHome = destination == MobileDestination.HOME,
            activeSection = semanticSection,
            regularConversationOpen = semanticConversationOpen,
            assistantOpen = showAgentChat,
        )
        val screen = if (showAgentChat) {
            if (assistantVisible) {
                element(
                    MahayanaAssistantSemanticProjection.AgentId,
                    "button",
                    "大乘助手",
                )
            }
            "assistant-chat"
        } else when (destination) {
            MobileDestination.HOME -> {
                element(
                    TestTags.AppShell,
                    "application",
                    "Fabushi",
                    action = FabushiAppAgentSurface.Action(setOf("pressKey")) { key ->
                        require(key?.trim()?.equals("BACK", ignoreCase = true) == true) { "unsupported_app_surface_key" }
                        onBackToGrokHome()
                    },
                )
                element(
                    TestTags.HomeSearchButton,
                    "button",
                    if (showHomeSearch) "关闭搜索" else "搜索对话",
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) {
                        showHomeSearch = !showHomeSearch
                        if (!showHomeSearch) homeSearchQuery = ""
                    },
                )
                if (showHomeSearch) {
                    element(
                        TestTags.HomeSearchField,
                        "textbox",
                        "搜索对话",
                        action = FabushiAppAgentSurface.Action(setOf("setValue")) { homeSearchQuery = it.orEmpty() },
                    )
                }
                element(TestTags.ProfileAvatar, "button", "个人菜单", action = FabushiAppAgentSurface.Action(setOf("invoke")) { showAddMenu = true })
                element(TestTags.AddButton, "button", "新建对话", action = FabushiAppAgentSurface.Action(setOf("invoke")) { showComposeMenu = true })
                if (assistantVisible) {
                    element(
                        MahayanaAssistantSemanticProjection.AgentId,
                        "button",
                        "大乘助手",
                        action = FabushiAppAgentSurface.Action(setOf("invoke")) { showAgentChat = true },
                    )
                }
                if (assistantUnreadVisible) {
                    element(
                        MahayanaAssistantSemanticProjection.UnreadAgentId,
                        "img",
                        MahayanaAssistantSemanticProjection.unreadName(state.assistantHasUnread),
                    )
                }
                if (showAddMenu) {
                    element(
                        TestTags.MarketplaceEntry,
                        "menuitem",
                        "插件市场",
                        action = FabushiAppAgentSurface.Action(setOf("invoke")) {
                            showAddMenu = false
                            destination = MobileDestination.MARKETPLACE
                        },
                    )
                    element(
                        TestTags.RemoteComputerEntry,
                        "menuitem",
                        "我的电脑",
                        action = FabushiAppAgentSurface.Action(setOf("invoke")) {
                            showAddMenu = false
                            destination = MobileDestination.REMOTE_COMPUTER
                        },
                    )
                }
                "home"
            }
            MobileDestination.MARKETPLACE -> {
                element(
                    TestTags.MarketplaceBack,
                    "button",
                    "返回消息",
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) { destination = MobileDestination.HOME },
                )
                element(
                    TestTags.SearchField,
                    "textbox",
                    "搜索插件",
                    action = FabushiAppAgentSurface.Action(setOf("setValue")) { onQueryChange(it.orEmpty()) },
                )
                element(
                    TestTags.SearchButton,
                    "button",
                    "搜索",
                    enabled = !state.loading,
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) { onSearch() },
                )
                element(TestTags.HostStatus, "status", state.message)
                state.plugins.take(100).forEach { plugin ->
                    element(TestTags.plugin(plugin.pluginId), "group", plugin.displayName)
                    element(
                        TestTags.open(plugin.pluginId),
                        "button",
                        "打开 ${plugin.displayName}",
                        action = FabushiAppAgentSurface.Action(setOf("invoke")) { onOpen(plugin) },
                    )
                    element(
                        TestTags.install(plugin.pluginId),
                        "button",
                        "安装 ${plugin.displayName}",
                        enabled = plugin.latestVersion != null && state.installingPluginId == null,
                        action = FabushiAppAgentSurface.Action(setOf("invoke")) { onInstall(plugin) },
                    )
                }
                "marketplace"
            }
            MobileDestination.REMOTE_COMPUTER -> {
                element(TestTags.RemoteComputerSurface, "application", "远程控制我的电脑")
                element(
                    TestTags.RemoteComputerClose,
                    "button",
                    "关闭远程控制",
                    action = FabushiAppAgentSurface.Action(setOf("invoke")) { destination = MobileDestination.HOME },
                )
                "remote-computer"
            }
        }
        if (state.permissionRequest != null) {
            element(
                TestTags.PermissionApprove,
                "button",
                "授权插件权限",
                action = FabushiAppAgentSurface.Action(setOf("invoke")) { onApprovePermissions() },
            )
            element(
                TestTags.PermissionDeny,
                "button",
                "拒绝插件权限",
                action = FabushiAppAgentSurface.Action(setOf("invoke")) { onDenyPermissions() },
            )
        }
        appAgentSurface?.publish(
            screen = if (state.permissionRequest != null) "permission-dialog" else screen,
            elements = elements,
            actions = actions,
        )
    }
    DisposableEffect(appAgentSurface) {
        onDispose { appAgentSurface?.clear() }
    }

    if (showAgentChat) {
        MobileAgentChat(
            state,
            onChatDraftChange,
            onSendChat,
            onStopChat,
            onOpenGeneratedMiniApp,
        ) { showAgentChat = false }
        return
    }

    state.permissionRequest?.let { request ->
        AlertDialog(
            modifier = Modifier.testTag(TestTags.PermissionDialog),
            onDismissRequest = onDenyPermissions,
            title = { Text("插件权限") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text("${request.pluginId} 请求以下权限：")
                    request.permissions.forEach { Text("• $it") }
                }
            },
            confirmButton = {
                Button(
                    onClick = onApprovePermissions,
                    modifier = Modifier.testTag(TestTags.PermissionApprove),
                ) { Text("授权") }
            },
            dismissButton = {
                OutlinedButton(
                    onClick = onDenyPermissions,
                    modifier = Modifier.testTag(TestTags.PermissionDeny),
                ) { Text("拒绝") }
            },
        )
    }

    when (destination) {
        MobileDestination.HOME -> ConversationHome(
            updateState = updateState,
            onCheckUpdate = onCheckUpdate,
            onInstallUpdate = onInstallUpdate,
            onOpenMarketplace = { destination = MobileDestination.MARKETPLACE },
            onOpenRemoteComputer = { destination = MobileDestination.REMOTE_COMPUTER },
            showAddMenu = showAddMenu,
            onShowAddMenuChange = { showAddMenu = it },
            showComposeMenu = showComposeMenu,
            onShowComposeMenuChange = { showComposeMenu = it },
            showSearch = showHomeSearch,
            searchQuery = homeSearchQuery,
            onShowSearchChange = { visible ->
                showHomeSearch = visible
                if (!visible) homeSearchQuery = ""
            },
            onSearchQueryChange = { homeSearchQuery = it },
            messagingState = messagingState,
            messagingActorId = messagingActorId,
            onMessagingRefresh = onMessagingRefresh,
            onCreateDirect = onCreateDirect,
            onCreateConversation = onCreateConversation,
            onSendText = onSendText,
            onSendAttachment = onSendAttachment,
            onSendVoice = onSendVoice,
            onLoadBlob = onLoadBlob,
            onSendContact = onSendContact,
            onSendPoll = onSendPoll,
            onVotePoll = onVotePoll,
            onSendLocation = onSendLocation,
            onEditText = onEditText,
            onDeleteMessage = onDeleteMessage,
            onSetMessagePinned = onSetMessagePinned,
            onSetReaction = onSetReaction,
            onForwardMessage = onForwardMessage,
            onStartTyping = onStartTyping,
            onStopTyping = onStopTyping,
            onSetPinned = onSetPinned,
            onSetArchived = onSetArchived,
            onSetMuted = onSetMuted,
            onMarkRead = onMarkRead,
            onSetMarkedUnread = onSetMarkedUnread,
            onSetDraft = onSetDraft,
            onUpdateConversationInfo = onUpdateConversationInfo,
            onSetConversationParticipant = onSetConversationParticipant,
            onRemoveConversationParticipant = onRemoveConversationParticipant,
            onUpsertFolder = onUpsertFolder,
            onDeleteFolder = onDeleteFolder,
            onOpenAgentChat = { showAgentChat = true },
            logoutBusy = state.logoutBusy,
            logoutError = state.logoutError,
            onLogout = onLogout,
            onSemanticContextChanged = { section, conversationOpen ->
                semanticSection = section
                semanticConversationOpen = conversationOpen
            },
        )
        MobileDestination.MARKETPLACE -> MarketplaceContent(
            state = state,
            onQueryChange = onQueryChange,
            onSearch = onSearch,
            onInstall = onInstall,
            onOpen = onOpen,
            onSubmitVariables = onSubmitPluginVariables,
            onCancelVariables = onCancelPluginVariables,
            onBack = { destination = MobileDestination.HOME },
        )
        MobileDestination.REMOTE_COMPUTER -> RemoteComputerSurface(
            onClose = { destination = MobileDestination.HOME },
        )
    }
}


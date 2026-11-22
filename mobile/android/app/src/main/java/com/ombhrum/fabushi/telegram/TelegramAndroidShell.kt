package com.ombhrum.fabushi.telegram

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
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
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ombhrum.fabushi.AndroidMediaViewer
import com.ombhrum.fabushi.AndroidUpdateUiState
import com.ombhrum.fabushi.ConversationKind
import com.ombhrum.fabushi.FabushiAppAgentSurface
import com.ombhrum.fabushi.MessagingUiState
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Primary 1:1 Telegram Android Jetpack Compose Client & Unified GPUI-Telegram / Fabushi / Mika Shell.
 */
@Composable
fun TelegramAndroidShell(
    telegramViewModel: TelegramViewModel = viewModel(),
    mikaBridgeCallbacks: MikaBridgeCallbacks? = null,
    mikaWorkspaceContent: (@Composable (onBack: () -> Unit) -> Unit)? = null,
    externalMessagingState: MessagingUiState? = null,
    updateState: AndroidUpdateUiState? = null,
    appAgentSurface: FabushiAppAgentSurface? = null,
    onCheckForUpdates: () -> Unit = {},
    onDownloadAndInstallUpdate: () -> Unit = {},
    onSwitchToGrokShell: () -> Unit = {},
    onSwitchToClassicFabushi: () -> Unit = {},
    onOpenMiniAppPluginId: (String) -> Unit = {},
) {
    val state by telegramViewModel.state.collectAsState()
    val drawerState = rememberDrawerState(initialValue = DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    var pendingCreateKind by remember { mutableStateOf<ConversationKind?>(null) }

    LaunchedEffect(mikaBridgeCallbacks) {
        telegramViewModel.attachMikaBridge(mikaBridgeCallbacks)
    }

    LaunchedEffect(externalMessagingState) {
        if (externalMessagingState != null) {
            telegramViewModel.syncExternalMessagingState(externalMessagingState)
        }
    }

    LaunchedEffect(state.toastBanner) {
        if (state.toastBanner != null) {
            delay(3200)
            telegramViewModel.clearToastBanner()
        }
    }

    // Publish semantic App MCP surface for deterministic native UI inspection & automation
    LaunchedEffect(state.subScreen, state.activeChatId, state.selectedFolderId, state.isLocked, state.chats.size) {
        val surface = appAgentSurface ?: return@LaunchedEffect
        publishTelegramSemanticSurface(surface, state, telegramViewModel)
    }

    ProvideTelegramTheme(settings = state.themeSettings) {
        val palette = LocalTelegramPalette.current

        Box(
            modifier = Modifier
                .fillMaxSize()
                .background(palette.appBackground)
                .testTag("telegram-android-shell-root"),
        ) {
            if (state.isLocked) {
                TelegramPasscodeScreen(
                    errorMessage = state.passcodeError,
                    biometricEnabled = state.biometricUnlockEnabled,
                    onVerifyPin = telegramViewModel::verifyPasscode,
                    onBiometricSuccess = telegramViewModel::unlockViaBiometricSuccess,
                )
            } else {
                ModalNavigationDrawer(
                    drawerState = drawerState,
                    gesturesEnabled = state.subScreen == TelegramSubScreen.HOME,
                    drawerContent = {
                        TelegramDrawerSheetContent(
                            state = state,
                            onToggleNightMode = telegramViewModel::toggleQuickNightMode,
                            onSwitchAccount = telegramViewModel::switchAccount,
                            onAddAccount = telegramViewModel::addAccount,
                            onOpenSavedMessages = {
                                scope.launch { drawerState.close() }
                                telegramViewModel.openSavedMessages()
                            },
                            onRequestCreateGroup = {
                                scope.launch { drawerState.close() }
                                pendingCreateKind = ConversationKind.GROUP
                            },
                            onRequestCreateSecretChat = {
                                scope.launch { drawerState.close() }
                                pendingCreateKind = ConversationKind.SECRET
                            },
                            onRequestCreateChannel = {
                                scope.launch { drawerState.close() }
                                pendingCreateKind = ConversationKind.CHANNEL
                            },
                            onNavigate = { targetScreen, targetPage ->
                                scope.launch { drawerState.close() }
                                telegramViewModel.navigateTo(targetScreen, targetPage)
                            },
                            onSwitchToGrokShell = {
                                scope.launch { drawerState.close() }
                                onSwitchToGrokShell()
                            },
                            onSwitchToClassicFabushi = {
                                scope.launch { drawerState.close() }
                                onSwitchToClassicFabushi()
                            },
                            onCloseDrawer = {
                                scope.launch { drawerState.close() }
                            },
                        )
                    },
                ) {
                    Column(modifier = Modifier.fillMaxSize()) {
                        // Floating Green VoIP Call Mini-Banner when Call is Minimized
                        if (state.callState.phase != CallPhase.IDLE && state.callState.isMinimized) {
                            TelegramCallOverlay(
                                callState = state.callState,
                                onToggleMute = telegramViewModel::toggleCallMute,
                                onToggleVideo = telegramViewModel::toggleCallVideo,
                                onToggleScreenShare = telegramViewModel::toggleCallScreenShare,
                                onToggleSpeaker = telegramViewModel::toggleCallSpeaker,
                                onMinimize = telegramViewModel::setCallMinimized,
                                onEndCall = telegramViewModel::endCall,
                            )
                        }

                        // Floating Live Mini App PiP Restoration Banner (Desktop v1.4/v1.6 in-place restore parity)
                        if (state.miniAppPip.active && state.miniAppPip.minimized && state.subScreen != TelegramSubScreen.GLOBAL_DHARMA) {
                            Row(
                                modifier = Modifier
                                    .fillMaxWidth()
                                    .background(Color(0xFF1A2233))
                                    .clickable { telegramViewModel.restoreMiniAppFromPip() }
                                    .padding(horizontal = 14.dp, vertical = 8.dp)
                                    .testTag("miniapp-live-pip-banner"),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(
                                    text = "🪷 ${state.miniAppPip.appTitle} 后台运行中 · ${state.miniAppPip.statusText.ifBlank { "点击原地恢复（不重置状态）" }}",
                                    color = Color(0xFFFFE082),
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.SemiBold,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.weight(1f),
                                )
                                TextButton(onClick = { telegramViewModel.restoreMiniAppFromPip() }) {
                                    Text("恢复", color = Color(0xFF64B5F6), fontSize = 12.sp, fontWeight = FontWeight.Bold)
                                }
                                TextButton(onClick = { telegramViewModel.dismissMiniAppPip() }) {
                                    Text("关闭", color = Color(0xFFFF8A80), fontSize = 12.sp)
                                }
                            }
                        }

                        Box(modifier = Modifier.weight(1f)) {
                            when (state.subScreen) {
                                TelegramSubScreen.HOME -> {
                                    TelegramHomeScreen(
                                        state = state,
                                        onOpenDrawer = { scope.launch { drawerState.open() } },
                                        onLockAppNow = telegramViewModel::lockAppNow,
                                        onSetGlobalSearch = telegramViewModel::setGlobalSearch,
                                        onSelectFolder = telegramViewModel::selectFolder,
                                        onOpenFolderSettings = {
                                            telegramViewModel.openSettingsPage(TelegramSettingsPage.CHAT_FOLDERS)
                                        },
                                        onToggleArchiveExpanded = telegramViewModel::toggleArchiveExpanded,
                                        onOpenStory = telegramViewModel::openStory,
                                        onCloseStory = telegramViewModel::closeStory,
                                        onCreateStory = telegramViewModel::createStory,
                                        onOpenChat = { id -> telegramViewModel.openChat(id) },
                                        onToggleChatSelection = telegramViewModel::toggleChatSelection,
                                        onClearChatSelection = telegramViewModel::clearChatSelection,
                                        onPinChat = telegramViewModel::togglePinChat,
                                        onArchiveChat = telegramViewModel::toggleArchiveChat,
                                        onMuteChat = telegramViewModel::toggleMuteChat,
                                        onToggleUnreadChat = telegramViewModel::toggleUnreadChat,
                                        onRequestCreateKind = { kind -> pendingCreateKind = kind },
                                    )
                                }

                                TelegramSubScreen.CHAT -> {
                                    val chat = state.activeChat
                                    if (chat == null) {
                                        LaunchedEffect(Unit) { telegramViewModel.closeChat() }
                                    } else {
                                        TelegramChatScreen(
                                            state = state,
                                            chat = chat,
                                            onBack = telegramViewModel::closeChat,
                                            onStartVoiceCall = { telegramViewModel.startCall(chat.id, chat.title, video = false) },
                                            onStartVideoCall = { isGroup -> telegramViewModel.startCall(chat.id, chat.title, video = true, groupCall = isGroup) },
                                            onOpenTalkModeOrb = { telegramViewModel.setTalkModeOrbVisible(true) },
                                            onSetInChatSearch = telegramViewModel::setInChatSearch,
                                            onToggleMuteChat = telegramViewModel::toggleMuteChat,
                                            onSetChatTtl = telegramViewModel::setChatTtl,
                                            onOpenSecretChatVerify = telegramViewModel::openSecretChatFingerprintDialog,
                                            onOpenChannelStats = telegramViewModel::openChannelStats,
                                            onExportChatHistory = telegramViewModel::exportChatHistory,
                                            onClearChatHistory = telegramViewModel::clearChatHistory,
                                            onSelectForumTopic = telegramViewModel::selectForumTopic,
                                            onCreateForumTopic = { title, emoji -> telegramViewModel.createForumTopic(chat.id, title, emoji) },
                                            onCyclePinnedBanner = telegramViewModel::cyclePinnedBanner,
                                            onDraftChange = { draft -> telegramViewModel.updateDraft(chat.id, draft) },
                                            onSetReplyTarget = telegramViewModel::setReplyTarget,
                                            onSetEditTarget = telegramViewModel::setEditTarget,
                                            onSendMessage = { txt, silent, sched, ttl ->
                                                telegramViewModel.sendComposerMessage(chat.id, txt, silent, sched, ttl)
                                            },
                                            onToggleRecordMode = telegramViewModel::toggleComposerRecordMode,
                                            onStartRecording = telegramViewModel::startVoiceOrVideoRecording,
                                            onDragRecording = telegramViewModel::updateRecordingDrag,
                                            onCancelRecording = telegramViewModel::cancelRecording,
                                            onFinishRecording = { telegramViewModel.finishAndSendRecording(chat.id) },
                                            onSendSticker = { emoji, pack -> telegramViewModel.sendSticker(chat.id, emoji, pack) },
                                            onSendGif = { gif -> telegramViewModel.sendGif(chat.id, gif) },
                                            onSendPollOrQuiz = { q, opts, anon, multi, quiz, corrIdx, exp ->
                                                telegramViewModel.sendPollOrQuiz(chat.id, q, opts, anon, multi, quiz, corrIdx, exp)
                                            },
                                            onVotePoll = { msgId, optId -> telegramViewModel.votePoll(chat.id, msgId, optId) },
                                            onSendLocation = { lat, lon, venue -> telegramViewModel.sendLocation(chat.id, lat, lon, venue) },
                                            onSendContact = { name, phone -> telegramViewModel.sendContactCard(chat.id, name, phone) },
                                            onSendMediaAttachment = { fn, mime, bytes -> telegramViewModel.sendMediaAttachment(chat.id, fn, mime, bytes) },
                                            onOpenMediaViewer = telegramViewModel::openMediaViewer,
                                            onPlayOrToggleAudio = telegramViewModel::playOrToggleAudioMessage,
                                            onSeekAudio = telegramViewModel::seekAudioPlayback,
                                            onCycleAudioSpeed = telegramViewModel::cycleAudioSpeed,
                                            onStopAudio = telegramViewModel::stopAudioPlayback,
                                            onToggleReaction = { msgId, emoji -> telegramViewModel.toggleReaction(chat.id, msgId, emoji) },
                                            onTogglePinMessage = { msgId -> telegramViewModel.togglePinMessage(chat.id, msgId) },
                                            onDeleteMessage = { msgId, forAll -> telegramViewModel.deleteMessage(chat.id, msgId, forAll) },
                                            onForwardMessage = telegramViewModel::forwardMessage,
                                            onTranslateMessage = { msgId -> telegramViewModel.translateMessage(chat.id, msgId) },
                                            onClickInlineButton = { btn, openUrl ->
                                                telegramViewModel.clickInlineButton(chat.id, btn, openUrl, onOpenMiniAppPluginId)
                                            },
                                            onRunWithMika = { prompt -> telegramViewModel.runWithMikaFromChat(chat.id, prompt) },
                                            onOpenGlobalDharmaMiniApp = { telegramViewModel.navigateTo(TelegramSubScreen.GLOBAL_DHARMA) },
                                            onOpenDiscussionSheet = telegramViewModel::openDiscussionSheet,
                                            onPostDiscussionComment = telegramViewModel::postDiscussionComment,
                                        )
                                    }
                                }

                                TelegramSubScreen.CONTACTS -> {
                                    TelegramContactsScreen(
                                        chats = state.chats,
                                        onOpenChat = { id -> telegramViewModel.openChat(id) },
                                        onCreateSecret = { pendingCreateKind = ConversationKind.SECRET },
                                        onCreateGroup = { pendingCreateKind = ConversationKind.GROUP },
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }

                                TelegramSubScreen.CALLS_HISTORY -> {
                                    TelegramCallsHistoryScreen(
                                        history = state.callHistory,
                                        onStartCall = { convId, peer, video -> telegramViewModel.startCall(convId, peer, video) },
                                        onOpenTalkMode = { telegramViewModel.setTalkModeOrbVisible(true) },
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }

                                TelegramSubScreen.BOTFATHER -> {
                                    BotFatherWorkspaceScreen(
                                        existingBots = state.chats.filter { it.isBot },
                                        codingBots = state.codingAgentBots,
                                        onCreateBot = telegramViewModel::createCompanionBot,
                                        onCreateCodingBot = telegramViewModel::createCodingAgentBot,
                                        onCycleCodingPermission = telegramViewModel::cycleCodingAgentPermission,
                                        onOpenBotChat = { id -> telegramViewModel.openChat(id) },
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }

                                TelegramSubScreen.GLOBAL_DHARMA -> {
                                    GlobalDharmaMiniAppScreen(
                                        dharmaState = state.globalDharma,
                                        onSelectCountry = telegramViewModel::selectDharmaCountry,
                                        onOfferDharma = telegramViewModel::offerDharmaToCountry,
                                        onRecordWheelSpin = telegramViewModel::recordPrayerWheelRevolution,
                                        onTapWoodenFish = telegramViewModel::tapWoodenFish,
                                        onCompletePractice = telegramViewModel::completePracticeMinutes,
                                        onAskDharmaAi = telegramViewModel::askDharmaAi,
                                        onMinimizeToPip = telegramViewModel::minimizeGlobalDharmaToPip,
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }

                                TelegramSubScreen.TON_WALLET -> {
                                    TonWalletScreen(
                                        walletState = state.tonWallet,
                                        onCreateNewWallet = telegramViewModel::createNewTonWallet,
                                        onImportMnemonic = telegramViewModel::importTonMnemonic,
                                        onSendTon = telegramViewModel::sendTonTransfer,
                                        onSwapTonToUsdt = telegramViewModel::swapTonToUsdt,
                                        onStakeTon = telegramViewModel::stakeTon,
                                        onEmulateTonConnect = telegramViewModel::emulateTonConnectUri,
                                        onResolveTonConnect = telegramViewModel::resolveTonConnect,
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }

                                TelegramSubScreen.HERMES_STUDIO -> {
                                    HermesStudioScreen(
                                        studioState = state.hermesStudio,
                                        onSelectTab = telegramViewModel::selectHermesTab,
                                        onBranchSession = telegramViewModel::branchHermesSession,
                                        onRunMoaPrompt = telegramViewModel::runHermesMoaPrompt,
                                        onSaveSoul = telegramViewModel::updateHermesSoul,
                                        onToggleSkill = telegramViewModel::toggleHermesSkill,
                                        onExecuteTerminal = telegramViewModel::executeHermesTerminal,
                                        onAddCronJob = telegramViewModel::addHermesCronJob,
                                        onAdvanceKanban = telegramViewModel::advanceKanbanCard,
                                        onDecideApproval = telegramViewModel::decideHermesApproval,
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }

                                TelegramSubScreen.MAGPIE_GATEWAY -> {
                                    MagpieGatewayScreen(
                                        gatewayState = state.magpieGateway,
                                        onSelectStrategy = telegramViewModel::setMagpieStrategy,
                                        onConfigureProvider = telegramViewModel::configureMagpieProvider,
                                        onImportEnvOrUri = telegramViewModel::importMagpieEnvOrUri,
                                        onTestPiiMask = telegramViewModel::testMagpiePiiMask,
                                        onSyncModelsDev = telegramViewModel::syncModelsDevCatalog,
                                        onScanLocalSubscriptions = telegramViewModel::scanLocalSubscriptions,
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }

                                TelegramSubScreen.CLOUD_COMPUTER -> {
                                    CloudComputerWorkbenchScreen(
                                        computerState = state.cloudComputer,
                                        onSelectNode = telegramViewModel::selectComputerNode,
                                        onAcquireLease = telegramViewModel::acquireCuaLease,
                                        onEmergencyTakeover = telegramViewModel::emergencyHumanTakeover,
                                        onExecuteOp = telegramViewModel::executeComputerOp,
                                        onAdvanceTaskFsm = telegramViewModel::advanceGlobalTaskFsm,
                                        onToggleRemoteAssist = telegramViewModel::toggleRemoteAssist,
                                        onUpdateAssistCode = telegramViewModel::updateRemoteAssistCode,
                                        onUpdateStreamSettings = telegramViewModel::updateRemoteStreamSettings,
                                        onIssueMcpGrant = telegramViewModel::issueRemoteMcpGrant,
                                        onRevokeMcpGrant = telegramViewModel::revokeRemoteMcpGrant,
                                        onOpenMikaWorkspace = { telegramViewModel.navigateTo(TelegramSubScreen.MIKA_WORKSPACE) },
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }

                                TelegramSubScreen.MIKA_WORKSPACE -> {
                                    if (mikaWorkspaceContent != null) {
                                        mikaWorkspaceContent {
                                            telegramViewModel.navigateTo(TelegramSubScreen.HOME)
                                        }
                                    } else {
                                        BuiltInMikaControlFallbackScreen(
                                            snapshot = state.mikaSnapshot,
                                            callbacks = mikaBridgeCallbacks,
                                            onDispatchTaskToChat = { prompt ->
                                                telegramViewModel.openChat("bot-mika-phone")
                                                telegramViewModel.runWithMikaFromChat("bot-mika-phone", prompt)
                                            },
                                            onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                        )
                                    }
                                }

                                TelegramSubScreen.SETTINGS -> {
                                    TelegramSettingsScreen(
                                        state = state,
                                        updateState = updateState,
                                        onSelectPage = telegramViewModel::openSettingsPage,
                                        onUpdateProfile = telegramViewModel::updateProfile,
                                        onUpdateTheme = telegramViewModel::updateThemeSettings,
                                        onConfigurePasscode = telegramViewModel::configurePasscode,
                                        onDisablePasscode = telegramViewModel::disablePasscode,
                                        onLockAppNow = telegramViewModel::lockAppNow,
                                        onClearLocalMediaCache = telegramViewModel::clearLocalMediaCache,
                                        onUpsertCustomFolder = telegramViewModel::upsertCustomFolder,
                                        onDeleteCustomFolder = telegramViewModel::deleteCustomFolder,
                                        onUpdatePowerAndNotifications = telegramViewModel::updatePowerAndNotificationPrefs,
                                        onUpdateLanguageAndTranslate = telegramViewModel::updateLanguageAndTranslate,
                                        onConfigureServerEndpoint = telegramViewModel::configureServerEndpoint,
                                        onOpenTonWallet = { telegramViewModel.navigateTo(TelegramSubScreen.TON_WALLET) },
                                        onCheckForUpdates = onCheckForUpdates,
                                        onDownloadAndInstallUpdate = onDownloadAndInstallUpdate,
                                        onInitSplitKeyVault = telegramViewModel::initSplitKeyVault,
                                        onRequestDeviceVaultSync = telegramViewModel::requestDeviceVaultSync,
                                        onBackToHome = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                    )
                                }
                            }
                        }
                    }
                }
            }

            // ================================================================
            // Global Modal Dialogs & Full-Screen Overlays
            // ================================================================

            CreateConversationOrSecretDialog(
                kind = pendingCreateKind,
                onDismiss = { pendingCreateKind = null },
                onCreateGroupOrChannel = telegramViewModel::createNewGroupOrChannel,
                onCreateSecretChat = telegramViewModel::createSecretChat,
            )

            TelegramSecretChatDialog(
                chat = state.secretChatDialogChatId?.let { id -> state.chats.firstOrNull { it.id == id } },
                onVerify = telegramViewModel::markSecretChatVerified,
                onDismiss = { telegramViewModel.openSecretChatFingerprintDialog(null) },
            )

            state.viewingMediaMessage?.let { mediaMsg ->
                AndroidMediaViewer(
                    message = mediaMsg.toLegacyChatMessage(),
                    onLoadBlob = telegramViewModel::loadMediaBlob,
                    onClose = { telegramViewModel.openMediaViewer(null) },
                )
            }

            if (state.talkOrbState.visible) {
                TelegramTalkModeOrbOverlay(
                    orbState = state.talkOrbState,
                    onBargeIn = telegramViewModel::triggerTalkModeBargeIn,
                    onSendUtterance = telegramViewModel::submitTalkModeUtterance,
                    onClose = { telegramViewModel.setTalkModeOrbVisible(false) },
                )
            }

            if (state.callState.phase != CallPhase.IDLE && !state.callState.isMinimized) {
                TelegramCallOverlay(
                    callState = state.callState,
                    onToggleMute = telegramViewModel::toggleCallMute,
                    onToggleVideo = telegramViewModel::toggleCallVideo,
                    onToggleScreenShare = telegramViewModel::toggleCallScreenShare,
                    onToggleSpeaker = telegramViewModel::toggleCallSpeaker,
                    onMinimize = telegramViewModel::setCallMinimized,
                    onEndCall = telegramViewModel::endCall,
                )
            }

            // Floating Telegram Toast Pill
            AnimatedVisibility(
                visible = !state.toastBanner.isNullOrBlank(),
                modifier = Modifier
                    .align(Alignment.BottomCenter)
                    .navigationBarsPadding()
                    .padding(bottom = 72.dp, start = 20.dp, end = 20.dp),
            ) {
                Surface(
                    shape = RoundedCornerShape(22.dp),
                    color = Color(0xDD1C2733),
                    shadowElevation = 6.dp,
                ) {
                    Text(
                        text = state.toastBanner.orEmpty(),
                        color = Color.White,
                        fontSize = 13.sp,
                        fontWeight = FontWeight.Medium,
                        modifier = Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
                    )
                }
            }
        }
    }
}

// ============================================================================
// Contacts & Calls History Screens + Built-in Mika Control Center Fallback
// ============================================================================

@Composable
private fun TelegramContactsScreen(
    chats: List<TelegramChatItem>,
    onOpenChat: (String) -> Unit,
    onCreateSecret: () -> Unit,
    onCreateGroup: () -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)
    var query by remember { mutableStateOf("") }
    val contacts = remember(chats, query) {
        chats.filter {
            (it.kind == ConversationKind.DIRECT || it.kind == ConversationKind.SECRET || it.isBot) &&
                (query.isBlank() || it.title.contains(query, ignoreCase = true) || it.subtitle.contains(query, ignoreCase = true))
        }
    }

    Column(modifier = Modifier.fillMaxSize().background(palette.appBackground)) {
        WorkspaceTopBar("联系人通讯录", "按在线状态排序 · 支持端到端加密聊天", TelegramGlyph.CONTACTS, onBack)
        LazyColumn(
            modifier = Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item {
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    placeholder = { Text("搜索联系人或 @机器人…") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                    Button(onClick = onCreateGroup, modifier = Modifier.weight(1f)) { Text("👥 新建群组") }
                    Button(onClick = onCreateSecret, modifier = Modifier.weight(1f)) { Text("🔒 新建加密聊天") }
                }
            }
            items(contacts, key = { it.id }) { c ->
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = palette.surfaceColor,
                    modifier = Modifier.fillMaxWidth().clickable { onOpenChat(c.id) },
                ) {
                    Row(modifier = Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        TelegramAvatar(idSeed = c.id, title = c.title, size = 44.dp, online = c.online, isSecret = c.isSecretChat, isCompanionGhost = c.isCompanionGhost)
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(c.title, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = palette.primaryText)
                            Text(if (c.online) "在线" else c.subtitle, fontSize = 12.sp, color = if (c.online) palette.primaryAccent else palette.secondaryText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun TelegramCallsHistoryScreen(
    history: List<CallHistoryEntry>,
    onStartCall: (String, String, Boolean) -> Unit,
    onOpenTalkMode: () -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)

    Column(modifier = Modifier.fillMaxSize().background(palette.appBackground)) {
        WorkspaceTopBar("通话与全双工语音", "端到端 333-Emoji 校验通话 & <60ms VAD 语音光球", TelegramGlyph.PHONE_CALL, onBack)
        LazyColumn(
            modifier = Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(12.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item {
                Button(onClick = onOpenTalkMode, modifier = Modifier.fillMaxWidth()) {
                    Text("🎙 启动 <60ms 全双工 AI 实时语音对话光球")
                }
            }
            items(history, key = { it.id }) { item ->
                Surface(shape = RoundedCornerShape(14.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                    Row(modifier = Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        TelegramAvatar(idSeed = item.conversationId, title = item.peerTitle, size = 44.dp)
                        Spacer(Modifier.width(12.dp))
                        Column(modifier = Modifier.weight(1f)) {
                            Text(item.peerTitle, fontWeight = FontWeight.Bold, fontSize = 15.sp, color = if (item.missed) palette.destructiveRed else palette.primaryText)
                            Text(
                                text = "${if (item.outgoing) "↗ 呼出" else "↙ 呼入"} · ${item.timeLabel} · ${item.durationLabel}",
                                fontSize = 12.sp,
                                color = palette.secondaryText,
                            )
                        }
                        Box(
                            modifier = Modifier.size(38.dp).clip(CircleShape).clickable { onStartCall(item.conversationId, item.peerTitle, item.isVideo) },
                            contentAlignment = Alignment.Center,
                        ) {
                            TelegramIcon(
                                glyph = if (item.isVideo) TelegramGlyph.VIDEO_CALL else TelegramGlyph.PHONE_CALL,
                                tint = palette.primaryAccent,
                                size = 20.dp,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun BuiltInMikaControlFallbackScreen(
    snapshot: MikaBridgeUiSnapshot,
    callbacks: MikaBridgeCallbacks?,
    onDispatchTaskToChat: (String) -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)
    var taskPrompt by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.appBackground)
            .testTag("telegram-mika-fallback-screen"),
    ) {
        WorkspaceTopBar(
            title = "Mika 本机操控中心",
            subtitle = "无障碍 UI 语义树 · 无线 ADB · 后台虚拟副屏 · Linux 沙箱",
            glyph = TelegramGlyph.MIKA_PHONE,
            onBack = onBack,
        )
        LazyColumn(
            modifier = Modifier.fillMaxSize().navigationBarsPadding(),
            contentPadding = PaddingValues(14.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("📱 端侧执行通道实时诊断", fontWeight = FontWeight.Bold, color = palette.primaryText)
                        MikaDiagRow("无障碍服务 (Accessibility UI Tree)", snapshot.accessibilityEnabled) { callbacks?.openAccessibilitySettings() }
                        MikaDiagRow("无线调试配对 (Wireless ADB Binder)", snapshot.wirelessAdbPaired || snapshot.helperBinderAlive) { callbacks?.openWirelessAdbPairing() }
                        MikaDiagRow("后台虚拟副屏 (Headless VirtualDisplay)", snapshot.virtualDisplayActive, null)
                        MikaDiagRow("Linux PRoot 终端沙箱", snapshot.sandboxReady) { callbacks?.openLinuxTerminal() }
                        Row(horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                            Text("灵动悬浮胶囊窗 (Floating Capsule)", fontSize = 13.sp, color = palette.primaryText)
                            Switch(checked = snapshot.capsuleVisible, onCheckedChange = { callbacks?.toggleFloatingCapsule(it) })
                        }
                    }
                }
            }

            snapshot.pendingConfirmationPrompt?.let { confirmText ->
                item {
                    Surface(shape = RoundedCornerShape(16.dp), color = palette.destructiveRed.copy(alpha = 0.12f), modifier = Modifier.fillMaxWidth()) {
                        Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Text("⚠️ 敏感操作安全确认 (ConfirmationGate)", fontWeight = FontWeight.Bold, color = palette.destructiveRed)
                            Text(confirmText, fontSize = 13.sp, color = palette.primaryText)
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Button(onClick = { callbacks?.resolveConfirmation(true) }) { Text("允许执行") }
                                Button(
                                    onClick = { callbacks?.resolveConfirmation(false) },
                                    colors = ButtonDefaults.buttonColors(containerColor = palette.destructiveRed, contentColor = Color.White),
                                ) { Text("立即拦截") }
                            }
                        }
                    }
                }
            }

            item {
                Surface(shape = RoundedCornerShape(16.dp), color = palette.surfaceColor, modifier = Modifier.fillMaxWidth()) {
                    Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text("🤖 下达手机操控指令（自动同步至 @MikaPhoneBot 对话）", fontWeight = FontWeight.Bold, color = palette.primaryText)
                        OutlinedTextField(
                            value = taskPrompt,
                            onValueChange = { taskPrompt = it },
                            placeholder = { Text("例如：打开系统设置查看存储空间，并把结果发回 Telegram") },
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Button(
                                onClick = {
                                    val p = taskPrompt.ifBlank { "检查本机无障碍与通知状态" }
                                    taskPrompt = ""
                                    onDispatchTaskToChat(p)
                                },
                                modifier = Modifier.weight(1f),
                            ) { Text("开始执行并回传") }
                            if (snapshot.activeTask != null) {
                                TextButton(onClick = { callbacks?.stopActiveTask() }) {
                                    Text("⏹ 停止", color = palette.destructiveRed)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MikaDiagRow(label: String, ready: Boolean, onConfigure: (() -> Unit)?) {
    val palette = LocalTelegramPalette.current
    Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
        Text("${if (ready) "🟢" else "🟡"} $label", fontSize = 13.sp, color = palette.primaryText, modifier = Modifier.weight(1f))
        if (onConfigure != null) {
            TextButton(onClick = onConfigure) { Text(if (ready) "查看" else "去开启", fontSize = 12.sp) }
        }
    }
}

private fun publishTelegramSemanticSurface(
    surface: FabushiAppAgentSurface,
    state: TelegramUiState,
    vm: TelegramViewModel,
) {
    val elements = ArrayList<FabushiAppAgentSurface.Element>(32)
    val actions = LinkedHashMap<String, FabushiAppAgentSurface.Action>(32)

    fun registerButton(id: String, name: String, onTap: () -> Unit) {
        val safeId = id.replace(Regex("[^A-Za-z0-9._:/@-]"), "_").take(180)
        if (elements.any { it.agentId == safeId }) return
        elements += FabushiAppAgentSurface.Element(agentId = safeId, role = "button", name = name.take(220))
        actions[safeId] = FabushiAppAgentSurface.Action(setOf("tap")) { onTap() }
    }

    registerButton("telegram.nav.home", "Telegram 消息列表") { vm.navigateTo(TelegramSubScreen.HOME) }
    registerButton("telegram.nav.saved", "打开收藏夹 Saved Messages") { vm.openSavedMessages() }
    registerButton("telegram.nav.dharma", "打开全球法布施小程序") { vm.navigateTo(TelegramSubScreen.GLOBAL_DHARMA) }
    registerButton("telegram.nav.wallet", "打开 TON 钱包") { vm.navigateTo(TelegramSubScreen.TON_WALLET) }
    registerButton("telegram.nav.hermes", "打开 Hermes AI 工作室") { vm.navigateTo(TelegramSubScreen.HERMES_STUDIO) }
    registerButton("telegram.nav.magpie", "打开 Magpie AI 网关") { vm.navigateTo(TelegramSubScreen.MAGPIE_GATEWAY) }
    registerButton("telegram.nav.computer", "打开云电脑控制台") { vm.navigateTo(TelegramSubScreen.CLOUD_COMPUTER) }
    registerButton("telegram.nav.mika", "打开 Mika 手机操控中心") { vm.navigateTo(TelegramSubScreen.MIKA_WORKSPACE) }
    registerButton("telegram.nav.settings", "打开 Telegram 设置") { vm.navigateTo(TelegramSubScreen.SETTINGS) }

    state.chats.take(15).forEach { chat ->
        registerButton("telegram.chat.open:${chat.id}", "进入会话 ${chat.title}") { vm.openChat(chat.id) }
    }

    state.activeChat?.let { active ->
        val inputId = "telegram.chat.composer:${active.id}".replace(Regex("[^A-Za-z0-9._:/@-]"), "_").take(180)
        elements += FabushiAppAgentSurface.Element(
            agentId = inputId,
            role = "textbox",
            name = "聊天输入框 ${active.title}".take(220),
            valuePresent = active.draftText.isNotEmpty(),
            valueLength = active.draftText.length,
        )
        actions[inputId] = FabushiAppAgentSurface.Action(setOf("set_text", "clear")) { text ->
            vm.updateDraft(active.id, text.orEmpty())
        }
        registerButton("telegram.chat.send:${active.id}", "发送消息到 ${active.title}") {
            vm.sendComposerMessage(active.id, active.draftText)
        }
    }

    runCatching {
        surface.publish(
            screen = "telegram.${state.subScreen.name.lowercase()}",
            elements = elements,
            actions = actions,
        )
    }
}

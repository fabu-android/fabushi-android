package com.ombhrum.fabushi.telegram

import android.content.Intent
import android.net.Uri
import android.provider.Settings
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
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
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

/**
 * Single Unified Apple iOS Messages Android Shell (`F12`, `F13`, `F15`):
 * - Zero `ModalNavigationDrawer`, zero `GROK`/`CLASSIC_FABUSHI` shell switching, zero overlapping FABs
 * - Persistent translucent 5-tab iOS bottom bar (`信息` / `联系人` / `智能体` / `远程` / `设置`) with crisp vector icons
 * - Live `LocalAppleUiStrings` localization (`zh-CN` <-> `en-US`)
 * - Preserves `internal fun publishTelegramSemanticSurface(...)` with canonical `"invoke"` / `"setValue"` actions
 */
@Composable
fun TelegramAndroidShell(
    telegramViewModel: TelegramViewModel = viewModel(),
    mikaBridgeCallbacks: MikaBridgeCallbacks? = null,
    mikaWorkspaceContent: (@Composable (onBackToTelegram: () -> Unit) -> Unit)? = null,
    externalMessagingState: MessagingUiState? = null,
    updateState: AndroidUpdateUiState? = null,
    appAgentSurface: FabushiAppAgentSurface? = null,
    onCheckForUpdates: () -> Unit = {},
    onDownloadAndInstallUpdate: () -> Unit = {},
    onOpenMiniAppPluginId: (String) -> Unit = {},
) {
    val state by telegramViewModel.state.collectAsState()
    val context = LocalContext.current

    var createDialogKind by remember { mutableStateOf<ConversationKind?>(null) }

    LaunchedEffect(mikaBridgeCallbacks) {
        telegramViewModel.attachMikaBridge(mikaBridgeCallbacks)
    }

    LaunchedEffect(state.toastBanner) {
        if (!state.toastBanner.isNullOrBlank()) {
            delay(2800)
            telegramViewModel.clearToastBanner()
        }
    }

    LaunchedEffect(
        appAgentSurface,
        state.subScreen,
        state.activeChatId,
        state.chats.size,
        state.activeChat?.draftText,
        state.selectedFolderId,
    ) {
        val surface = appAgentSurface ?: return@LaunchedEffect
        publishTelegramSemanticSurface(surface = surface, state = state, vm = telegramViewModel)
    }

    val openUrl: (String) -> Unit = { url ->
        runCatching {
            context.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        }
    }

    val uiStrings = remember(state.uiLanguage) { resolveUiStrings(state.uiLanguage) }
    val totalUnread = remember(state.chats) { state.chats.filter { !it.isArchived }.sumOf { it.unreadCount } }

    ProvideTelegramTheme(settings = state.themeSettings) {
        CompositionLocalProvider(LocalAppleUiStrings provides uiStrings) {
            val palette = LocalTelegramPalette.current

            // Passcode Lock Gate
            if (state.isLocked) {
                TelegramPasscodeScreen(
                    errorMessage = state.passcodeError,
                    biometricEnabled = state.biometricUnlockEnabled,
                    onVerifyPin = telegramViewModel::verifyPasscode,
                    onBiometricSuccess = telegramViewModel::unlockViaBiometricSuccess,
                )
                return@CompositionLocalProvider
            }

            // Auth Wizard Gate when signed out and not in offline mode
            if (!state.isAuthenticated && !state.offlineLocalMode) {
                TelegramAuthWizardScreen(
                    serverEndpoint = state.serverEndpoint,
                    isAuthenticating = state.isAuthenticating,
                    authError = state.authError,
                    alipayLoginUrl = state.alipayLoginUrl,
                    alipayStatusText = state.alipayStatusText,
                    emailCodeSentNotice = state.emailCodeSentNotice,
                    hasPendingTwoFactor = !state.pendingTwoFactorChallengeToken.isNullOrBlank(),
                    pendingTwoFactorHint = state.pendingTwoFactorHint,
                    onUpdateServerEndpoint = telegramViewModel::updateDesktopServerEndpoint,
                    onPasswordLogin = telegramViewModel::loginWithPassword,
                    onPasswordRegister = telegramViewModel::registerWithPassword,
                    onSubmitTwoFactor = telegramViewModel::submitTwoFactorCloudPassword,
                    onCancelTwoFactor = telegramViewModel::cancelTwoFactorChallenge,
                    onSendEmailCode = telegramViewModel::sendEmailVerificationCode,
                    onEmailRegister = telegramViewModel::registerWithEmail,
                    onStartAlipayLogin = telegramViewModel::startAlipayLogin,
                    onPollAlipayLogin = { telegramViewModel.pollAlipayLogin(silent = false) },
                    onToggleTheme = telegramViewModel::toggleAuthThemeMode,
                    onToggleLanguage = telegramViewModel::toggleAuthLanguage,
                    onEnterOfflineLocalMode = telegramViewModel::enterOfflineLocalMode,
                )
                return@CompositionLocalProvider
            }

            val showBottomTabBar = when (state.subScreen) {
                TelegramSubScreen.HOME,
                TelegramSubScreen.CONTACTS,
                TelegramSubScreen.BOTFATHER -> true
                TelegramSubScreen.SETTINGS -> state.settingsPage == TelegramSettingsPage.OVERVIEW
                else -> false
            }

            val activeTab = when (state.subScreen) {
                TelegramSubScreen.CONTACTS -> AppleShellTab.CONTACTS
                TelegramSubScreen.BOTFATHER,
                TelegramSubScreen.HERMES_STUDIO,
                TelegramSubScreen.MIKA_WORKSPACE,
                TelegramSubScreen.CLOUD_COMPUTER -> AppleShellTab.AGENTS
                TelegramSubScreen.SETTINGS,
                TelegramSubScreen.MAGPIE_GATEWAY -> AppleShellTab.SETTINGS
                else -> AppleShellTab.MESSAGES
            }

            Box(
                modifier = Modifier
                    .fillMaxSize()
                    .background(palette.appBackground),
            ) {
                Column(modifier = Modifier.fillMaxSize()) {
                    // Minimized VoIP Banner (honest unavailable notice if ever non-IDLE)
                    if (state.callState.phase != CallPhase.IDLE && state.callState.isMinimized) {
                        TelegramCallOverlay(
                            callState = state.callState,
                            onToggleMute = telegramViewModel::toggleCallMute,
                            onToggleSpeaker = telegramViewModel::toggleCallSpeaker,
                            onToggleVideo = telegramViewModel::toggleCallVideo,
                            onToggleScreenShare = telegramViewModel::toggleCallScreenShare,
                            onToggleMinimize = { telegramViewModel.setCallMinimized(!state.callState.isMinimized) },
                            onEndCall = telegramViewModel::endCall,
                        )
                    }

                    // Minimized MiniApp PiP Banner (crisp vector icon, zero raw emoji)
                    if (state.miniAppPip.minimized && state.subScreen != TelegramSubScreen.GLOBAL_DHARMA) {
                        Surface(
                            color = palette.cardBg,
                            shadowElevation = 2.dp,
                            modifier = Modifier
                                .fillMaxWidth()
                                .statusBarsPadding()
                                .clickable { telegramViewModel.restoreMiniAppFromPip() }
                                .testTag("telegram-miniapp-pip-banner"),
                        ) {
                            Row(
                                modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                TelegramIcon(TelegramGlyph.DHARMA_WHEEL, tint = palette.goldMerit, size = 18.dp)
                                Spacer(Modifier.width(8.dp))
                                Text(
                                    text = "${state.miniAppPip.appTitle} · ${state.miniAppPip.statusText}",
                                    color = palette.primaryText,
                                    fontSize = 12.sp,
                                    fontWeight = FontWeight.Medium,
                                    maxLines = 1,
                                    overflow = TextOverflow.Ellipsis,
                                    modifier = Modifier.weight(1f),
                                )
                                TextButton(onClick = telegramViewModel::dismissMiniAppPip) {
                                    Text(uiStrings.done, color = palette.topBarActionColor, fontSize = 12.sp)
                                }
                            }
                        }
                        TelegramHairlineDivider()
                    }

                    // Main Content Area
                    Box(modifier = Modifier.weight(1f).fillMaxWidth()) {
                        when (state.subScreen) {
                            TelegramSubScreen.HOME -> {
                                TelegramHomeScreen(
                                    state = state,
                                    onSelectFolder = telegramViewModel::selectFolder,
                                    onToggleArchiveExpanded = telegramViewModel::toggleArchiveExpanded,
                                    onGlobalSearchChange = telegramViewModel::setGlobalSearch,
                                    onOpenChat = { chatId -> telegramViewModel.openChat(chatId) },
                                    onToggleSelectChat = telegramViewModel::toggleChatSelection,
                                    onClearSelection = telegramViewModel::clearChatSelection,
                                    onTogglePinChat = telegramViewModel::togglePinChat,
                                    onToggleArchiveChat = telegramViewModel::toggleArchiveChat,
                                    onToggleMuteChat = telegramViewModel::toggleMuteChat,
                                    onToggleUnreadChat = telegramViewModel::toggleUnreadChat,
                                    onDeleteChat = telegramViewModel::leaveChat,
                                    onCreateDialogKind = { kind -> createDialogKind = kind },
                                    onOpenBotFather = { telegramViewModel.navigateTo(TelegramSubScreen.BOTFATHER) },
                                    onAddContactByUsername = { u -> telegramViewModel.addContactByUsername(u) },
                                    onLockAppNow = telegramViewModel::lockAppNow,
                                    onRefreshChats = telegramViewModel::refreshAllFromHost,
                                )
                            }

                            TelegramSubScreen.CHAT -> {
                                val activeChat = state.activeChat
                                if (activeChat == null) {
                                    LaunchedEffect(Unit) { telegramViewModel.closeChat() }
                                } else {
                                    val chatMessages = state.messagesByChat[activeChat.id].orEmpty()
                                    val chatTopics = state.topicsByChat[activeChat.id].orEmpty()
                                    TelegramChatScreen(
                                        state = state,
                                        chat = activeChat,
                                        messages = chatMessages,
                                        topics = chatTopics,
                                        totalUnreadCount = totalUnread,
                                        onBack = telegramViewModel::closeChat,
                                        onSelectTopic = telegramViewModel::selectForumTopic,
                                        onSetInChatSearch = telegramViewModel::setInChatSearch,
                                        onUpdateDraft = { draft -> telegramViewModel.updateDraft(activeChat.id, draft) },
                                        onSetReplyTarget = telegramViewModel::setReplyTarget,
                                        onSetEditTarget = telegramViewModel::setEditTarget,
                                        onSendMessage = { text, silent, sched, ttl ->
                                            telegramViewModel.sendComposerMessage(activeChat.id, text, silent, sched, ttl)
                                        },
                                        onStartBotChat = { telegramViewModel.startBotChat(activeChat.id) },
                                        onOpenMiniApp = { pluginId ->
                                            if (pluginId == "global_dharma" || pluginId == "fabushi-global-dharma") {
                                                telegramViewModel.navigateTo(TelegramSubScreen.GLOBAL_DHARMA)
                                            } else {
                                                onOpenMiniAppPluginId(pluginId)
                                            }
                                        },
                                        onRetryFailedMessage = { msgId ->
                                            telegramViewModel.retryFailedMessage(activeChat.id, msgId)
                                        },
                                        onLoadOlderMessages = {
                                            telegramViewModel.loadOlderMessages(activeChat.id)
                                        },
                                        onToggleReaction = { msgId, emoji ->
                                            telegramViewModel.toggleReaction(activeChat.id, msgId, emoji)
                                        },
                                        onTogglePinMessage = { msgId ->
                                            telegramViewModel.togglePinMessage(activeChat.id, msgId)
                                        },
                                        onCyclePinnedBanner = { telegramViewModel.cyclePinnedBanner(activeChat.id) },
                                        onDeleteMessage = { msgId, everyone ->
                                            telegramViewModel.deleteMessage(activeChat.id, msgId, everyone)
                                        },
                                        onForwardMessage = telegramViewModel::forwardMessage,
                                        onTranslateMessage = { msgId ->
                                            telegramViewModel.translateMessage(activeChat.id, msgId)
                                        },
                                        onSendSticker = { emoji, pack ->
                                            telegramViewModel.sendSticker(activeChat.id, emoji, pack)
                                        },
                                        onSendGif = { gif -> telegramViewModel.sendGif(activeChat.id, gif) },
                                        onSendPoll = { q, opts, anon, multi, quiz, corr, expl ->
                                            telegramViewModel.sendPollOrQuiz(activeChat.id, q, opts, anon, multi, quiz, corr, expl)
                                        },
                                        onVotePoll = { msgId, optId ->
                                            telegramViewModel.votePoll(activeChat.id, msgId, optId)
                                        },
                                        onSendLocation = { lat, lon, title ->
                                            telegramViewModel.sendLocation(activeChat.id, lat, lon, title)
                                        },
                                        onSendContact = { name, phone ->
                                            telegramViewModel.sendContactCard(activeChat.id, name, phone)
                                        },
                                        onSendMedia = { name, mime, bytes ->
                                            telegramViewModel.sendMediaAttachment(activeChat.id, name, mime, bytes)
                                        },
                                        onOpenMediaViewer = telegramViewModel::openMediaViewer,
                                        onClickInlineButton = { msgId, btn ->
                                            telegramViewModel.clickInlineButton(
                                                conversationId = activeChat.id,
                                                button = btn,
                                                onOpenUrl = openUrl,
                                                onOpenMiniAppId = onOpenMiniAppPluginId,
                                                messageId = msgId,
                                            )
                                        },
                                        onResolveMikaConfirmation = { approved, threadId, askId ->
                                            telegramViewModel.resolveMikaConfirmation(approved, threadId, askId)
                                        },
                                        onSetChatTtl = { ttl -> telegramViewModel.setChatTtl(activeChat.id, ttl) },
                                        onClearChatHistory = { telegramViewModel.clearChatHistory(activeChat.id) },
                                        onLeaveChat = { telegramViewModel.leaveChat(activeChat.id) },
                                        onToggleMuteChat = { telegramViewModel.toggleMuteChat(activeChat.id) },
                                        onTogglePinChat = { telegramViewModel.togglePinChat(activeChat.id) },
                                        onExportChatHistory = { fmt -> telegramViewModel.exportChatHistory(activeChat.id, fmt) },
                                        onOpenSecretVerify = { telegramViewModel.openSecretChatFingerprintDialog(activeChat.id) },
                                        onOpenChannelStats = { telegramViewModel.openChannelStats(activeChat.id) },
                                        onCloseChannelStats = { telegramViewModel.openChannelStats(null) },
                                        onOpenDiscussion = telegramViewModel::openDiscussionSheet,
                                        onPostDiscussionComment = telegramViewModel::postDiscussionComment,
                                        onToggleRecordMode = telegramViewModel::toggleComposerRecordMode,
                                        onStartRecording = telegramViewModel::startVoiceOrVideoRecording,
                                        onDragRecording = telegramViewModel::updateRecordingDrag,
                                        onCancelRecording = telegramViewModel::cancelRecording,
                                        onFinishRecording = { telegramViewModel.finishAndSendRecording(activeChat.id) },
                                        onPlayOrToggleAudio = telegramViewModel::playOrToggleAudioMessage,
                                        onCycleAudioSpeed = telegramViewModel::cycleAudioSpeed,
                                        onStopAudio = telegramViewModel::stopAudioPlayback,
                                        onSetAttachmentSheetOpen = telegramViewModel::setComposerAttachmentSheetOpen,
                                        onPatchFabushiBot = { id, name, desc, shape, color, model, appr ->
                                            telegramViewModel.patchFabushiBot(
                                                botId = id,
                                                displayName = name,
                                                description = desc,
                                                avatarShape = shape,
                                                avatarColor = color,
                                                preferredModel = model,
                                                approvalMode = appr,
                                            )
                                        },
                                        onBindBotComputer = { botId, nodeId, clear ->
                                            telegramViewModel.setBotComputer(botId, nodeId, clear)
                                        },
                                        onRefreshBotComputer = telegramViewModel::getBotComputer,
                                        onToast = telegramViewModel::showToastBanner,
                                    )
                                }
                            }

                            TelegramSubScreen.CONTACTS -> {
                                TelegramContactsScreen(
                                    contacts = state.serverContacts,
                                    chats = state.chats,
                                    isAuthenticated = state.isAuthenticated,
                                    onOpenContact = { contact ->
                                        val existingChat = contact.chatId?.toString()?.let { cid ->
                                            state.chats.firstOrNull { it.id == cid }
                                        } ?: state.chats.firstOrNull { it.peerUserId == contact.userId }
                                        val resolvedId = existingChat?.id ?: contact.chatId?.toString()
                                        if (resolvedId != null) {
                                            telegramViewModel.openChat(
                                                chatId = resolvedId,
                                                provisionalTitle = contact.displayName.ifBlank { contact.username },
                                                provisionalUsername = contact.username,
                                                provisionalPeerUserId = contact.userId,
                                                provisionalBio = contact.bio.ifBlank { null },
                                            )
                                        } else {
                                            telegramViewModel.addContactByUsername(contact.username.ifBlank { contact.displayName })
                                        }
                                    },
                                    onAddContact = { username -> telegramViewModel.addContactByUsername(username) },
                                    onDeleteContact = { uid -> telegramViewModel.deleteContact(uid) },
                                    onRefreshContacts = telegramViewModel::refreshContacts,
                                    onNewGroup = { createDialogKind = ConversationKind.GROUP },
                                    onNewSecret = { createDialogKind = ConversationKind.SECRET },
                                )
                            }

                            TelegramSubScreen.BOTFATHER,
                            TelegramSubScreen.HERMES_STUDIO -> {
                                BotFatherWorkspaceScreen(
                                    existingBots = state.chats.filter { it.isBot },
                                    fabushiBots = state.fabushiBots,
                                    mikaSnapshot = state.mikaSnapshot,
                                    lastCreatedBotToken = state.lastCreatedBotToken,
                                    isAuthenticated = state.isAuthenticated,
                                    onOpenBot = telegramViewModel::openBotChat,
                                    onOpenBotChat = { id -> telegramViewModel.openChat(id) },
                                    onOpenBotByUsername = { u, name, desc ->
                                        telegramViewModel.openBotByUsername(username = u, displayName = name, description = desc)
                                    },
                                    onOpenMikaWorkspace = { telegramViewModel.navigateTo(TelegramSubScreen.MIKA_WORKSPACE) },
                                    onCreateBot = { name, desc, handle ->
                                        telegramViewModel.createFabushiBot(
                                            displayName = name,
                                            username = handle,
                                            description = desc,
                                            openAfterCreate = true,
                                        )
                                    },
                                    onBotFatherNlCreate = { prompt ->
                                        telegramViewModel.botfatherNlCreate(prompt = prompt, openAfterCreate = true)
                                    },
                                    onRefreshBots = telegramViewModel::refreshServerBots,
                                    onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                )
                            }

                            TelegramSubScreen.SETTINGS,
                            TelegramSubScreen.MAGPIE_GATEWAY -> {
                                val effectiveState = if (state.subScreen == TelegramSubScreen.MAGPIE_GATEWAY) {
                                    state.copy(settingsPage = TelegramSettingsPage.AI_PROVIDERS)
                                } else state
                                TelegramSettingsScreen(
                                    state = effectiveState,
                                    updateState = updateState,
                                    onSelectPage = { p ->
                                        telegramViewModel.navigateTo(TelegramSubScreen.SETTINGS, p)
                                    },
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
                                    onOpenGlobalDharma = { telegramViewModel.navigateTo(TelegramSubScreen.GLOBAL_DHARMA) },
                                    onSetMagpieStrategy = telegramViewModel::setMagpieStrategy,
                                    onConfigureAiProvider = { id, key -> telegramViewModel.configureMagpieProvider(id, key) },
                                    onSyncModelsDev = telegramViewModel::syncModelsDevCatalog,
                                    onCheckForUpdates = onCheckForUpdates,
                                    onDownloadAndInstallUpdate = onDownloadAndInstallUpdate,
                                    onInitSplitKeyVault = telegramViewModel::initSplitKeyVault,
                                    onRequestDeviceVaultSync = telegramViewModel::requestDeviceVaultSync,
                                    onRefreshSessions = telegramViewModel::refreshSessions,
                                    onRevokeSession = telegramViewModel::revokeSession,
                                    onRevokeOtherSessions = telegramViewModel::revokeOtherSessions,
                                    onSignOut = telegramViewModel::signOutToAuthWizard,
                                    onBackToHome = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
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
                                    onBack = { telegramViewModel.navigateTo(TelegramSubScreen.SETTINGS) },
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
                                    onBack = { telegramViewModel.navigateTo(TelegramSubScreen.SETTINGS) },
                                )
                            }

                            TelegramSubScreen.MIKA_WORKSPACE,
                            TelegramSubScreen.CLOUD_COMPUTER -> {
                                if (mikaWorkspaceContent != null) {
                                    mikaWorkspaceContent {
                                        telegramViewModel.navigateTo(TelegramSubScreen.BOTFATHER)
                                    }
                                } else {
                                    BuiltInMikaControlFallbackScreen(
                                        snapshot = state.mikaSnapshot,
                                        onRefresh = telegramViewModel::refreshMikaSnapshot,
                                        onBack = { telegramViewModel.navigateTo(TelegramSubScreen.BOTFATHER) },
                                    )
                                }
                            }

                            TelegramSubScreen.CALLS_HISTORY -> {
                                TelegramCallsHistoryScreen(
                                    onBack = { telegramViewModel.navigateTo(TelegramSubScreen.HOME) },
                                )
                            }
                        }
                    }

                    // Persistent 4-Tab Mobile Bottom Bar (Desktop v1.6.9 aligned — zero desktop Remote tab)
                    if (showBottomTabBar) {
                        ApplePersistentBottomTabBar(
                            activeTab = activeTab,
                            unreadMessagesCount = totalUnread,
                            onSelectTab = { tab ->
                                when (tab) {
                                    AppleShellTab.MESSAGES -> telegramViewModel.navigateTo(TelegramSubScreen.HOME)
                                    AppleShellTab.CONTACTS -> telegramViewModel.navigateTo(TelegramSubScreen.CONTACTS)
                                    AppleShellTab.AGENTS -> telegramViewModel.navigateTo(TelegramSubScreen.BOTFATHER)
                                    AppleShellTab.SETTINGS -> telegramViewModel.navigateTo(
                                        TelegramSubScreen.SETTINGS,
                                        TelegramSettingsPage.OVERVIEW,
                                    )
                                }
                            },
                        )
                    }
                }

                // Root Overlays
                CreateConversationOrSecretDialog(
                    kind = createDialogKind,
                    onDismiss = { createDialogKind = null },
                    onCreateGroupOrChannel = telegramViewModel::createNewGroupOrChannel,
                    onCreateSecretChat = telegramViewModel::createSecretChat,
                )

                val secretChat = state.secretChatDialogChatId?.let { id -> state.chats.firstOrNull { it.id == id } }
                TelegramSecretChatDialog(
                    chat = secretChat,
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

                TelegramTalkModeOrbOverlay(
                    orbState = state.talkOrbState,
                    onClose = { telegramViewModel.setTalkModeOrbVisible(false) },
                    onSimulateBargeIn = telegramViewModel::triggerTalkModeBargeIn,
                    onSubmitUtterance = telegramViewModel::submitTalkModeUtterance,
                )

                if (state.callState.phase != CallPhase.IDLE && !state.callState.isMinimized) {
                    TelegramCallOverlay(
                        callState = state.callState,
                        onToggleMute = telegramViewModel::toggleCallMute,
                        onToggleSpeaker = telegramViewModel::toggleCallSpeaker,
                        onToggleVideo = telegramViewModel::toggleCallVideo,
                        onToggleScreenShare = telegramViewModel::toggleCallScreenShare,
                        onToggleMinimize = { telegramViewModel.setCallMinimized(!state.callState.isMinimized) },
                        onEndCall = telegramViewModel::endCall,
                    )
                }

                // Toast Notification Pill (respects bottom tab bar clearance)
                state.toastBanner?.let { banner ->
                    Surface(
                        shape = RoundedCornerShape(14.dp),
                        color = if (palette.isDark) Color(0xEE2C2C2E) else Color(0xEE1C1C1C),
                        shadowElevation = 8.dp,
                        modifier = Modifier
                            .align(Alignment.BottomCenter)
                            .navigationBarsPadding()
                            .padding(horizontal = 16.dp, vertical = if (showBottomTabBar) 74.dp else 72.dp),
                    ) {
                        Text(
                            text = banner,
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
}

@Composable
private fun ApplePersistentBottomTabBar(
    activeTab: AppleShellTab,
    unreadMessagesCount: Int,
    onSelectTab: (AppleShellTab) -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current

    Column(
        modifier = Modifier
            .fillMaxWidth()
            .background(palette.topBarBackground.copy(alpha = 0.96f))
            .navigationBarsPadding()
            .testTag("apple-bottom-nav-bar"),
    ) {
        TelegramHairlineDivider()
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(54.dp)
                .padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val items = listOf(
                Triple(AppleShellTab.MESSAGES, TelegramGlyph.CHAT_BUBBLE, strings.tabMessages),
                Triple(AppleShellTab.CONTACTS, TelegramGlyph.CONTACTS, strings.tabContacts),
                Triple(AppleShellTab.AGENTS, TelegramGlyph.BOT_ROBOT, strings.tabAgents),
                Triple(AppleShellTab.SETTINGS, TelegramGlyph.SETTINGS_GEAR, strings.tabSettings),
            )
            items.forEach { (tab, glyph, label) ->
                val selected = activeTab == tab
                val tint = if (selected) palette.primaryAccent else palette.secondaryText
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .clip(RoundedCornerShape(10.dp))
                        .clickable { onSelectTab(tab) }
                        .padding(vertical = 4.dp)
                        .testTag("apple-tab-${tab.name.lowercase()}"),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(2.dp),
                ) {
                    Box {
                        TelegramIcon(glyph = glyph, tint = tint, size = 22.dp)
                        if (tab == AppleShellTab.MESSAGES && unreadMessagesCount > 0) {
                            Box(
                                modifier = Modifier
                                    .align(Alignment.TopEnd)
                                    .clip(CircleShape)
                                    .background(palette.unreadBadgeActive)
                                    .padding(horizontal = 4.dp, vertical = 1.dp),
                            ) {
                                Text(
                                    text = if (unreadMessagesCount > 99) "99+" else "$unreadMessagesCount",
                                    color = palette.unreadBadgeContent,
                                    fontSize = 9.sp,
                                    fontWeight = FontWeight.Bold,
                                )
                            }
                        }
                    }
                    Text(
                        text = label,
                        color = tint,
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

@Composable
private fun TelegramContactsScreen(
    contacts: List<ServerContactItem>,
    chats: List<TelegramChatItem>,
    isAuthenticated: Boolean,
    onOpenContact: (ServerContactItem) -> Unit,
    onAddContact: (String) -> Unit,
    onDeleteContact: (String) -> Unit,
    onRefreshContacts: () -> Unit,
    onNewGroup: () -> Unit,
    onNewSecret: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val strings = LocalAppleUiStrings.current
    var showAddDialog by remember { mutableStateOf(false) }
    var usernameDraft by remember { mutableStateOf("") }

    Column(
        modifier = Modifier
            .fillMaxSize()
            .background(palette.groupedBackground)
            .testTag("telegram-contacts-screen"),
    ) {
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
                    text = strings.tabContacts,
                    color = palette.topBarContent,
                    fontSize = 28.sp,
                    fontWeight = FontWeight.Bold,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                    TextButton(onClick = onRefreshContacts) {
                        Text(if (strings.isEnglish) "Refresh" else "刷新", color = palette.topBarActionColor)
                    }
                    Box(
                        modifier = Modifier
                            .size(32.dp)
                            .clip(CircleShape)
                            .clickable { showAddDialog = true },
                        contentAlignment = Alignment.Center,
                    ) {
                        TelegramIcon(TelegramGlyph.PLUS, tint = palette.topBarActionColor, size = 20.dp)
                    }
                }
            }
        }
        TelegramHairlineDivider()

        LazyColumn(
            modifier = Modifier.weight(1f),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Surface(
                    shape = RoundedCornerShape(12.dp),
                    color = palette.modalBg,
                    modifier = Modifier
                        .fillMaxWidth()
                        .border(0.5.dp, palette.borderColor, RoundedCornerShape(12.dp)),
                ) {
                    Column {
                        ContactQuickActionRow(TelegramGlyph.PLUS, palette.primaryAccent, strings.addContactAction) { showAddDialog = true }
                        TelegramHairlineDivider(Modifier.padding(start = 52.dp))
                        ContactQuickActionRow(TelegramGlyph.CONTACTS, Color(0xFF34C759), strings.newGroupAction, onNewGroup)
                        TelegramHairlineDivider(Modifier.padding(start = 52.dp))
                        ContactQuickActionRow(TelegramGlyph.SECRET_SHIELD, Color(0xFF30B0C7), strings.newSecretChatAction, onNewSecret)
                    }
                }
            }

            if (contacts.isEmpty()) {
                item {
                    Surface(
                        shape = RoundedCornerShape(14.dp),
                        color = palette.modalBg,
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Column(
                            modifier = Modifier.padding(24.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            Text(
                                text = if (isAuthenticated) {
                                    if (strings.isEnglish) "No contacts yet" else "暂无联系人"
                                } else {
                                    if (strings.isEnglish) "Sign in to sync contacts" else "登录服务器账号后同步联系人"
                                },
                                color = palette.primaryText,
                                fontWeight = FontWeight.SemiBold,
                                fontSize = 15.sp,
                            )
                            Text(
                                text = if (strings.isEnglish) "Tap + above to add someone by @username." else "点击右上角「+」通过 @用户名添加联系人并发起私聊。",
                                color = palette.secondaryText,
                                fontSize = 12.sp,
                            )
                        }
                    }
                }
            } else {
                itemsIndexed(contacts, key = { index, c -> "contact_${index}_${c.userId}" }) { _, c ->
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        color = palette.modalBg,
                        modifier = Modifier
                            .fillMaxWidth()
                            .border(0.5.dp, palette.borderColor, RoundedCornerShape(12.dp)),
                    ) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { onOpenContact(c) }
                                .padding(horizontal = 12.dp, vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            TelegramAvatar(
                                idSeed = c.userId.toString(),
                                title = c.displayName.ifBlank { c.username },
                                size = 42.dp,
                            )
                            Spacer(Modifier.width(12.dp))
                            Column(Modifier.weight(1f)) {
                                Text(c.displayName.ifBlank { c.username }, color = palette.primaryText, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
                                val sub = listOfNotNull(
                                    c.username.takeIf { it.isNotBlank() }?.let { if (it.startsWith("@")) it else "@$it" },
                                    c.bio.takeIf { it.isNotBlank() },
                                ).joinToString(" · ")
                                if (sub.isNotBlank()) {
                                    Text(sub, color = palette.secondaryText, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                            }
                            TextButton(onClick = { onDeleteContact(c.userId.toString()) }) {
                                Text(strings.delete, color = palette.destructiveRed, fontSize = 12.sp)
                            }
                        }
                    }
                }
            }
        }
    }

    if (showAddDialog) {
        AlertDialog(
            onDismissRequest = { showAddDialog = false },
            containerColor = palette.modalBg,
            titleContentColor = palette.primaryText,
            textContentColor = palette.primaryText,
            title = { Text(strings.addContactAction) },
            text = {
                OutlinedTextField(
                    value = usernameDraft,
                    onValueChange = { usernameDraft = it },
                    label = { Text(if (strings.isEnglish) "@username or phone" else "@用户名或手机号") },
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
                    enabled = usernameDraft.isNotBlank(),
                    onClick = {
                        onAddContact(usernameDraft.trim())
                        usernameDraft = ""
                        showAddDialog = false
                    },
                    colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                ) {
                    Text(if (strings.isEnglish) "Add" else "添加")
                }
            },
            dismissButton = {
                TextButton(onClick = { showAddDialog = false }) {
                    Text(strings.cancel, color = palette.topBarActionColor)
                }
            },
        )
    }
}

@Composable
private fun ContactQuickActionRow(
    glyph: TelegramGlyph,
    badgeColor: Color,
    title: String,
    onClick: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 14.dp, vertical = 11.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            modifier = Modifier
                .size(28.dp)
                .clip(RoundedCornerShape(7.dp))
                .background(badgeColor),
            contentAlignment = Alignment.Center,
        ) {
            TelegramIcon(glyph = glyph, tint = Color.White, size = 15.dp)
        }
        Spacer(Modifier.width(12.dp))
        Text(title, color = palette.primaryText, fontSize = 15.sp, modifier = Modifier.weight(1f))
        TelegramIcon(TelegramGlyph.CHEVRON_RIGHT, tint = palette.hintText, size = 14.dp)
    }
}

@Composable
private fun TelegramCallsHistoryScreen(onBack: () -> Unit) {
    val palette = LocalTelegramPalette.current
    BackHandler(onBack = onBack)
    Column(modifier = Modifier.fillMaxSize().background(palette.groupedBackground)) {
        WorkspaceTopBar("通话记录", "音视频通话服务暂不可用", TelegramGlyph.PHONE_CALL, onBack)
        Box(modifier = Modifier.fillMaxSize().padding(28.dp), contentAlignment = Alignment.Center) {
            Text(
                text = "当前服务端未开启原生 WebRTC 音视频通话，暂无通话记录。",
                color = palette.secondaryText,
                fontSize = 14.sp,
            )
        }
    }
}

@Composable
private fun BuiltInMikaControlFallbackScreen(
    snapshot: MikaBridgeUiSnapshot,
    onRefresh: () -> Unit,
    onBack: () -> Unit,
) {
    val palette = LocalTelegramPalette.current
    val context = LocalContext.current
    BackHandler(onBack = onBack)

    Column(modifier = Modifier.fillMaxSize().background(palette.groupedBackground)) {
        WorkspaceTopBar("Mika 手机操控工作台", if (snapshot.available) "已连接" else "待配置", TelegramGlyph.MIKA_PHONE, onBack)
        LazyColumn(
            modifier = Modifier.weight(1f),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item {
                Surface(
                    shape = RoundedCornerShape(14.dp),
                    color = palette.modalBg,
                    modifier = Modifier.fillMaxWidth(),
                ) {
                    Column(modifier = Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        MikaDiagRow("无障碍服务", snapshot.accessibilityEnabled) {
                            runCatching { context.startActivity(Intent(Settings.ACTION_ACCESSIBILITY_SETTINGS)) }
                        }
                        MikaDiagRow("无线 ADB 配对", snapshot.wirelessAdbPaired, null)
                        MikaDiagRow("Linux PRoot 沙盒", snapshot.sandboxReady, null)
                        MikaDiagRow("MCP LAN 服务", snapshot.mcpLanEnabled, null)
                        Button(
                            onClick = onRefresh,
                            colors = ButtonDefaults.buttonColors(containerColor = palette.primaryAccent, contentColor = Color.White),
                            modifier = Modifier.fillMaxWidth(),
                        ) {
                            Text("刷新设备状态")
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
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(
                modifier = Modifier
                    .size(9.dp)
                    .clip(CircleShape)
                    .background(if (ready) palette.secretGreen else palette.unreadBadgeMuted),
            )
            Text(label, fontSize = 14.sp, color = palette.primaryText)
        }
        if (onConfigure != null) {
            TextButton(onClick = onConfigure) {
                Text(if (ready) "查看" else "去开启", color = palette.topBarActionColor, fontSize = 12.sp)
            }
        } else {
            Text(
                text = if (ready) "已就绪" else "未开启",
                color = if (ready) palette.statusGreenText else palette.secondaryText,
                fontSize = 12.sp,
            )
        }
    }
}

internal fun publishTelegramSemanticSurface(
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
        actions[safeId] = FabushiAppAgentSurface.Action(setOf("invoke")) { onTap() }
    }

    registerButton("telegram.nav.home", "信息") { vm.navigateTo(TelegramSubScreen.HOME) }
    registerButton("telegram.nav.contacts", "联系人") { vm.navigateTo(TelegramSubScreen.CONTACTS) }
    registerButton("telegram.nav.agents", "智能体") { vm.navigateTo(TelegramSubScreen.BOTFATHER) }
    registerButton("telegram.nav.saved", "收藏夹") { vm.openSavedMessages() }
    registerButton("telegram.nav.dharma", "全球法布施") { vm.navigateTo(TelegramSubScreen.GLOBAL_DHARMA) }
    registerButton("telegram.nav.wallet", "TON 钱包") { vm.navigateTo(TelegramSubScreen.TON_WALLET) }
    registerButton("telegram.nav.hermes", "Hermes Studio") { vm.navigateTo(TelegramSubScreen.HERMES_STUDIO) }
    registerButton("telegram.nav.magpie", "Magpie AI Gateway") { vm.navigateTo(TelegramSubScreen.MAGPIE_GATEWAY) }
    registerButton("telegram.nav.computer", "远程控制中心") { vm.navigateTo(TelegramSubScreen.CLOUD_COMPUTER) }
    registerButton("telegram.nav.mika", "Mika 手机操控工作台") { vm.navigateTo(TelegramSubScreen.MIKA_WORKSPACE) }
    registerButton("telegram.nav.settings", "设置") { vm.navigateTo(TelegramSubScreen.SETTINGS) }

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
        actions[inputId] = FabushiAppAgentSurface.Action(setOf("setValue")) { text ->
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

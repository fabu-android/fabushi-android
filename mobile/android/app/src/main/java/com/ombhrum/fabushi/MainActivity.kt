package com.ombhrum.fabushi

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ombhrum.fabushi.core.MahayanaHost
import com.ombhrum.fabushi.telegram.TelegramAndroidShell
import com.ombhrum.fabushi.telegram.TelegramSubScreen
import com.ombhrum.fabushi.telegram.TelegramViewModel
import com.wangzan101.mika.ui.EmbeddedMikaWorkspace
import kotlinx.coroutines.flow.MutableSharedFlow
import org.json.JSONObject

enum class MainShellMode {
    TELEGRAM,
    GROK,
    CLASSIC_FABUSHI,
}

class MainActivity : ComponentActivity() {
    private val deepLinks = MutableSharedFlow<Uri>(replay = 1, extraBufferCapacity = 31)
    private val tonConnectLinks = MutableSharedFlow<String>(replay = 1, extraBufferCapacity = 15)
    private val updateModel: AndroidUpdateViewModel by viewModels()
    private val appAgentSurface = FabushiAppAgentSurface()
    private lateinit var remoteDeviceGateway: FabushiRemoteDeviceGateway

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val ciBootstrapActive = FabushiCiBootstrap.prepare(this)
        remoteDeviceGateway = FabushiRemoteDeviceGateway(
            context = applicationContext,
            surface = appAgentSurface,
            metadata = FabushiCiBootstrap.gatewayMetadata(intent, ciBootstrapActive),
            configuredDeviceName = FabushiCiBootstrap.configuredDeviceName(intent, ciBootstrapActive),
            phoneBridge = FabushiApplication.bridgeOrNull(applicationContext),
        )
        val initialShellMode = when (intent?.getStringExtra("fabushi.shell.mode")?.lowercase()) {
            "grok" -> MainShellMode.GROK
            "classic", "legacy", "fabushi" -> MainShellMode.CLASSIC_FABUSHI
            else -> MainShellMode.TELEGRAM
        }

        enableEdgeToEdge()
        setContent {
            MaterialTheme {
                val model: MarketplaceViewModel = viewModel()
                val messagingModel: MessagingViewModel = viewModel()
                val botModel: MobileBotViewModel = viewModel()
                val telegramModel: TelegramViewModel = viewModel()

                val state by model.state.collectAsState()
                val messagingState by messagingModel.state.collectAsState()
                val botState by botModel.state.collectAsState()
                val updateState by updateModel.state.collectAsState()

                val mikaBridge = remember { FabushiApplication.bridgeOrNull(applicationContext) }
                var openedMiniApp by remember { mutableStateOf<MarketplacePlugin?>(null) }
                var shellMode by rememberSaveable { mutableStateOf(initialShellMode) }

                BackHandler(enabled = shellMode != MainShellMode.TELEGRAM && openedMiniApp == null) {
                    shellMode = MainShellMode.TELEGRAM
                }

                LaunchedEffect(model) {
                    deepLinks.collect { uri -> model.handleDeepLink(uri) }
                }

                LaunchedEffect(telegramModel) {
                    tonConnectLinks.collect { tcUri ->
                        shellMode = MainShellMode.TELEGRAM
                        telegramModel.navigateTo(TelegramSubScreen.TON_WALLET)
                        telegramModel.emulateTonConnectUri(tcUri)
                    }
                }

                LaunchedEffect(state.loggedIn) {
                    remoteDeviceGateway.setLoggedIn(state.loggedIn)
                    if (state.loggedIn) {
                        messagingModel.refresh()
                        model.refresh()
                        botModel.refreshBots()
                        telegramModel.refreshAllFromHost()
                    }
                }

                LaunchedEffect(shellMode, state.loggedIn) {
                    if (shellMode == MainShellMode.GROK && state.loggedIn) {
                        model.refresh()
                        botModel.refreshBots()
                    }
                }

                LaunchedEffect(state.browserLaunchNonce, state.browserLoginUrl) {
                    val loginUrl = state.browserLoginUrl
                    if (state.browserLaunchNonce > 0 && !loginUrl.isNullOrBlank() && !loginUrl.startsWith("about:blank")) {
                        CustomTabsIntent.Builder()
                            .setShowTitle(true)
                            .build()
                            .launchUrl(this@MainActivity, Uri.parse(loginUrl))
                    }
                }

                val active = openedMiniApp
                if (active != null) {
                    val miniAppTransportHost = remember(active.pluginId) { MahayanaHost(applicationContext) }
                    val miniAppPlatformBridge = remember(active.pluginId, miniAppTransportHost) {
                        MiniAppPlatformBridge(miniAppTransportHost)
                    }
                    DisposableEffect(miniAppTransportHost) {
                        onDispose { miniAppTransportHost.close() }
                    }
                    Box {
                        MiniAppWebMcpSurface(
                            plugin = active,
                            loadLocalHtml = { pluginId ->
                                model.loadLocalMiniAppHtml(pluginId) ?: globalDharmaHostShell(active)
                            },
                            callRuntimeToolJson = { pluginId, name, argumentsJson ->
                                if (pluginId == MiniAppPlatformBridge.GLOBAL_DHARMA_ID) {
                                    miniAppPlatformBridge.callOfficialMcpTool(
                                        pluginId = pluginId,
                                        name = name,
                                        arguments = JSONObject(argumentsJson.ifBlank { "{}" }),
                                    ).toString()
                                } else {
                                    model.callRuntimeToolJson(pluginId, name, argumentsJson)
                                }
                            },
                            onClose = { openedMiniApp = null },
                        )
                        if (active.pluginId == MiniAppPlatformBridge.GLOBAL_DHARMA_ID) {
                            GlobalDharmaCommercePanel(
                                modifier = Modifier
                                    .align(Alignment.BottomCenter)
                                    .testTag("global-dharma-commerce-panel"),
                            )
                        }
                    }
                } else {
                    when (shellMode) {
                        MainShellMode.TELEGRAM -> {
                            TelegramAndroidShell(
                                telegramViewModel = telegramModel,
                                mikaBridgeCallbacks = mikaBridge,
                                mikaWorkspaceContent = { onBackToTelegram ->
                                    EmbeddedMikaWorkspace(
                                        showBackToTelegramChip = true,
                                        onBackToTelegram = onBackToTelegram,
                                    )
                                },
                                externalMessagingState = messagingState,
                                updateState = updateState,
                                appAgentSurface = appAgentSurface,
                                onCheckForUpdates = { updateModel.checkForUpdates(force = true) },
                                onDownloadAndInstallUpdate = updateModel::downloadAndInstall,
                                onSwitchToGrokShell = { shellMode = MainShellMode.GROK },
                                onSwitchToClassicFabushi = { shellMode = MainShellMode.CLASSIC_FABUSHI },
                                onOpenMiniAppPluginId = { pluginId ->
                                    val resolved = state.plugins.firstOrNull { it.pluginId == pluginId }
                                        ?: MarketplacePlugin(
                                            pluginId = pluginId,
                                            displayName = if (pluginId == MiniAppPlatformBridge.GLOBAL_DHARMA_ID) "全球法布施" else pluginId,
                                            description = "Telegram Mini App · WebMCP Runtime",
                                            latestVersion = "2.0.0",
                                        )
                                    openedMiniApp = resolved
                                },
                            )
                        }

                        MainShellMode.GROK -> {
                            val miniAppBot = botState.activeBot?.takeIf { !it.miniAppId.isNullOrBlank() }
                            val miniAppPlugin = miniAppBot?.miniAppId?.let { id -> state.plugins.firstOrNull { it.pluginId == id } }
                            LaunchedEffect(miniAppBot?.id, miniAppPlugin?.pluginId, miniAppBot?.menuButtonText) {
                                if (miniAppBot != null) {
                                    appAgentSurface.setOverlay(
                                        key = "miniapp-bot-menu",
                                        elements = listOf(
                                            FabushiAppAgentSurface.Element(
                                                agentId = "mobile-bot-open-miniapp",
                                                role = "button",
                                                name = miniAppBot.menuButtonText ?: "打开应用",
                                                enabled = miniAppPlugin != null,
                                            ),
                                        ),
                                        actions = if (miniAppPlugin != null) {
                                            mapOf(
                                                "mobile-bot-open-miniapp" to FabushiAppAgentSurface.Action(setOf("invoke")) {
                                                    openedMiniApp = miniAppPlugin
                                                },
                                            )
                                        } else {
                                            emptyMap()
                                        },
                                    )
                                } else {
                                    appAgentSurface.clearOverlay("miniapp-bot-menu")
                                }
                            }
                            Box {
                                GrokMobileShellAndroid(
                                    accountName = state.accountName,
                                    messagingState = messagingState,
                                    botState = botState,
                                    appAgentSurface = appAgentSurface,
                                    onOpenLegacy = { shellMode = MainShellMode.CLASSIC_FABUSHI },
                                    onRefreshBots = botModel::refreshBots,
                                    onCreateBot = botModel::createBot,
                                    onOpenBot = botModel::openBot,
                                    onCloseBot = botModel::closeBot,
                                    onDraftChange = botModel::setDraft,
                                    onSend = botModel::send,
                                    onStop = botModel::stop,
                                )
                                if (miniAppBot != null) {
                                    Button(
                                        onClick = { miniAppPlugin?.let { openedMiniApp = it } },
                                        enabled = miniAppPlugin != null,
                                        modifier = Modifier
                                            .align(Alignment.TopEnd)
                                            .padding(top = 12.dp, end = 12.dp)
                                            .testTag("mobile-bot-open-miniapp"),
                                    ) {
                                        Text(miniAppBot.menuButtonText ?: "打开应用")
                                    }
                                }
                            }
                        }

                        MainShellMode.CLASSIC_FABUSHI -> {
                            FabushiScreen(
                                state = state,
                                onQueryChange = model::setQuery,
                                onSearch = model::refresh,
                                onInstall = model::install,
                                onOpen = { openedMiniApp = it },
                                onApprovePermissions = model::approvePermissions,
                                onDenyPermissions = model::denyPermissions,
                                updateState = updateState,
                                onCheckUpdate = { updateModel.checkForUpdates(force = true) },
                                onInstallUpdate = updateModel::downloadAndInstall,
                                messagingState = messagingState,
                                messagingActorId = messagingModel.currentActorId,
                                onMessagingRefresh = messagingModel::refresh,
                                onCreateDirect = messagingModel::createDirect,
                                onCreateConversation = messagingModel::createConversation,
                                onSendText = { conversationId, text, replyTo, silent, scheduledAt ->
                                    messagingModel.sendText(conversationId, text, replyTo, silent, scheduledAt)
                                },
                                onSendAttachment = messagingModel::sendAttachment,
                                onSendVoice = messagingModel::sendVoice,
                                onLoadBlob = messagingModel::loadBlob,
                                onSendContact = messagingModel::sendContact,
                                onSendPoll = messagingModel::sendPoll,
                                onVotePoll = messagingModel::votePoll,
                                onSendLocation = { conversationId, latitude, longitude ->
                                    messagingModel.sendLocation(conversationId, latitude, longitude)
                                },
                                onEditText = messagingModel::editText,
                                onDeleteMessage = { conversationId, messageId ->
                                    messagingModel.deleteMessage(conversationId, messageId)
                                },
                                onSetMessagePinned = messagingModel::setMessagePinned,
                                onSetReaction = messagingModel::setReaction,
                                onForwardMessage = messagingModel::forwardMessage,
                                onStartTyping = messagingModel::startTyping,
                                onStopTyping = messagingModel::stopTyping,
                                onSetPinned = messagingModel::setPinned,
                                onSetArchived = messagingModel::setArchived,
                                onSetMuted = messagingModel::setMuted,
                                onMarkRead = messagingModel::markRead,
                                onSetMarkedUnread = messagingModel::setMarkedUnread,
                                onSetDraft = messagingModel::setDraft,
                                onUpdateConversationInfo = messagingModel::updateConversationInfo,
                                onSetConversationParticipant = messagingModel::setConversationParticipant,
                                onRemoveConversationParticipant = messagingModel::removeConversationParticipant,
                                onUpsertFolder = messagingModel::upsertFolder,
                                onDeleteFolder = messagingModel::deleteFolder,
                                appAgentSurface = appAgentSurface,
                                authGateEnabled = false,
                                onAdvanceOnboarding = model::advanceOnboarding,
                                onSkipOnboarding = model::skipOnboarding,
                                onBeginBrowserLogin = model::beginBrowserLogin,
                                onReopenBrowserLogin = model::reopenBrowserLogin,
                                onCancelBrowserLogin = model::cancelBrowserLogin,
                                onLogout = model::logout,
                                onExitLegacy = { shellMode = MainShellMode.TELEGRAM },
                                onChatDraftChange = model::setChatDraft,
                                onSendChat = model::sendChat,
                                onStopChat = model::stopChat,
                            )
                        }
                    }
                }
            }
        }
        intent?.data?.let(::enqueueDeepLink)
    }

    override fun onStart() {
        super.onStart()
        updateModel.setForeground(true)
    }

    override fun onStop() {
        updateModel.setForeground(false)
        super.onStop()
    }

    override fun onDestroy() {
        if (::remoteDeviceGateway.isInitialized) remoteDeviceGateway.close()
        super.onDestroy()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        intent.data?.let(::enqueueDeepLink)
    }

    internal fun appAgentSurfaceForTesting(): FabushiAppAgentSurface = appAgentSurface

    private fun enqueueDeepLink(uri: Uri) {
        when (uri.scheme) {
            "fabushi" -> deepLinks.tryEmit(uri)
            "tc" -> tonConnectLinks.tryEmit(uri.toString())
        }
    }
}

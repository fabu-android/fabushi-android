package com.ombhrum.fabushi

import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidPresentationDeepLink
import com.ombhrum.fabushi.androidpreload.runtime.AndroidPresentationRuntimePort
import kotlinx.coroutines.flow.SharedFlow
import org.json.JSONObject

private enum class RendererRoute { GROK_HOME, AGENT_NETWORK, MESSAGING }

/**
 * Shipping production renderer corresponding to Grok ProductionRenderer.
 *
 * Android lifecycle/bootstrap stays outside this file. This renderer owns presentation
 * composition and delegates domain operations through ViewModels / the typed coordinator bridge.
 */
@Composable
internal fun ProductionRenderer(
    activity: ComponentActivity,
    deepLinks: SharedFlow<AndroidPresentationDeepLink>,
    updateModel: AndroidUpdateViewModel,
    runtimePort: AndroidPresentationRuntimePort,
) {
            val appAgentSurface = runtimePort.appAgentSurface

            MaterialTheme {
                val model: MarketplaceViewModel = viewModel()
                val messagingModel: MessagingViewModel = viewModel()
                val botModel: MobileBotViewModel = viewModel()
                val state by model.state.collectAsState()
                val messagingState by messagingModel.state.collectAsState()
                val botState by botModel.state.collectAsState()
                val updateState by updateModel.state.collectAsState()
                var openedMiniApp by remember { mutableStateOf<MarketplacePlugin?>(null) }
                var rendererRoute by remember { mutableStateOf(RendererRoute.GROK_HOME) }
                var commandPaletteOpen by remember { mutableStateOf(false) }
                var commandPaletteAgentFilter by remember { mutableStateOf<String?>(null) }
                var showAbout by remember { mutableStateOf(false) }
                var deepLinkInfo by remember { mutableStateOf<DeepLinkInfo?>(null) }

                BackHandler(enabled = rendererRoute != RendererRoute.GROK_HOME && state.loggedIn) {
                    rendererRoute = RendererRoute.GROK_HOME
                }

                LaunchedEffect(model) {
                    deepLinks.collect { link ->
                        if (link is AndroidDeepLink.Info) {
                            deepLinkInfo = link.toDeepLinkInfo()
                        } else {
                            model.handleDeepLink(link)
                        }
                    }
                }
                DeepLinkInfoDialog(
                    link = deepLinkInfo,
                    onClose = { deepLinkInfo = null },
                )
                if (showAbout) {
                    AndroidAboutDialog(
                        updateState = updateState,
                        onClose = { showAbout = false },
                    )
                }

                LaunchedEffect(state.loggedIn) {
                    runtimePort.setLoggedIn(state.loggedIn)
                    if (state.loggedIn) {
                        messagingModel.refresh()
                        model.refresh()
                        botModel.refreshBots()
                    }
                    if (!state.loggedIn) {
                        botModel.resetAccountScope()
                        rendererRoute = RendererRoute.GROK_HOME
                    }
                }
                LaunchedEffect(rendererRoute, state.loggedIn) {
                    if (state.loggedIn && (rendererRoute == RendererRoute.GROK_HOME || rendererRoute == RendererRoute.AGENT_NETWORK)) {
                        model.refresh()
                        botModel.refreshBots()
                    }
                }
                LaunchedEffect(state.browserLaunchNonce, state.browserLoginUrl) {
                    val loginUrl = state.browserLoginUrl
                    if (state.browserLaunchNonce > 0 && !loginUrl.isNullOrBlank()) {
                        runtimePort.launchExternalAuth(loginUrl)
                    }
                }

                val active = openedMiniApp
                if (active != null) {
                    val coordinator = remember { CoordinatorClient.presentation() }
                    val miniAppPlatformBridge = remember(active.pluginId, coordinator) {
                        MiniAppPlatformBridge(coordinator)
                    }
                    Box {
                        MiniAppWebMcpSurface(
                            plugin = active,
                            coordinator = coordinator,
                            loadLocalHtml = { pluginId ->
                                model.loadLocalMiniAppHtml(pluginId) ?: globalDharmaHostShell(active)
                            },
                            callRuntimeToolJson = { pluginId, name, argumentsJson, requestId ->
                                if (pluginId == MiniAppPlatformBridge.GLOBAL_DHARMA_ID) {
                                    miniAppPlatformBridge.callOfficialMcpTool(
                                        pluginId = pluginId,
                                        name = name,
                                        arguments = JSONObject(argumentsJson.ifBlank { "{}" }),
                                        requestId = requestId,
                                    ).toString()
                                } else {
                                    model.callRuntimeToolJson(pluginId, name, argumentsJson, requestId)
                                }
                            },
                            cancelRuntimeCall = { requestId ->
                                model.cancelRuntimeToolCall(requestId)
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
                } else if (state.onboardingStep >= 3 && state.authResolved && state.loggedIn && rendererRoute == RendererRoute.AGENT_NETWORK) {
                    AgentNetworkSurface(
                        bots = botState.bots,
                        appAgentSurface = appAgentSurface,
                        onBack = { rendererRoute = RendererRoute.GROK_HOME },
                        onOpenBot = { bot ->
                            rendererRoute = RendererRoute.GROK_HOME
                            botModel.openBot(bot)
                        },
                    )
                } else if (state.onboardingStep >= 3 && state.authResolved && state.loggedIn && rendererRoute == RendererRoute.GROK_HOME) {
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
                    botState.asyncTasksAgent?.let { agent ->
                        AgentAsyncTasksPanel(
                            agent = agent,
                            tasks = botState.asyncTasks,
                            loading = botState.asyncTasksLoading,
                            error = botState.asyncTasksError,
                            onClose = botModel::closeAsyncTasks,
                            onRefresh = botModel::refreshAsyncTasks,
                        )
                    }
                    val paletteAgentNames = buildMap {
                        put("mahayana-assistant", "Mahayana")
                        botState.bots.forEach { bot -> put(bot.id, bot.name) }
                    }
                    val paletteMessageEntries = commandPaletteMessageEntries(
                        messages = botState.paletteMessageSearch.value,
                        agentNames = paletteAgentNames,
                    ) { message ->
                        val targetBot = if (message.agentId == "mahayana-assistant") {
                            MobileBotSummaryAndroid(
                                id = "mahayana-assistant",
                                name = "Mahayana",
                                description = "Mahayana multi-step agent",
                            )
                        } else {
                            botState.bots.firstOrNull { it.id == message.agentId }
                        }
                        if (targetBot != null) {
                            rendererRoute = RendererRoute.GROK_HOME
                            botModel.openBot(targetBot, targetEntryId = message.entryId)
                        }
                    }
                    val paletteRoutineEntries = commandPaletteRoutineEntries(
                        routines = botState.paletteRoutines.value.filter { routine ->
                            commandPaletteAgentFilter == null || routine.agentId == commandPaletteAgentFilter
                        },
                        agentNames = paletteAgentNames,
                    ) { agentId ->
                        val targetBot = if (agentId == "mahayana-assistant") {
                            MobileBotSummaryAndroid(
                                id = "mahayana-assistant",
                                name = "Mahayana",
                                description = "Mahayana multi-step agent",
                            )
                        } else {
                            botState.bots.firstOrNull { it.id == agentId }
                        }
                        if (targetBot != null) {
                            rendererRoute = RendererRoute.GROK_HOME
                            botModel.openBot(targetBot)
                        }
                    }
                    val paletteEntries = buildList {
                        addAll(paletteMessageEntries)
                        addAll(paletteRoutineEntries)
                        botState.bots.forEach { bot ->
                            add(
                                CommandPaletteEntry(
                                    id = "agent:${bot.id}",
                                    kind = if (bot.isGroup) {
                                        CommandPaletteEntryKind.GROUP
                                    } else {
                                        CommandPaletteEntryKind.AGENT
                                    },
                                    label = bot.name,
                                    detail = bot.description.takeIf(String::isNotBlank),
                                    searchText = listOf(bot.name, bot.description, "agent bot")
                                        .joinToString(" "),
                                    isHidden = bot.isHidden,
                                    activate = {
                                        rendererRoute = RendererRoute.GROK_HOME
                                        botModel.openBot(bot)
                                    },
                                ),
                            )
                        }
                        add(
                            CommandPaletteEntry(
                                id = "android:messages",
                                kind = CommandPaletteEntryKind.COMMAND,
                                label = "Messages",
                                detail = "Open conversations",
                                searchText = "Messages conversations chats channels",
                                activate = { rendererRoute = RendererRoute.MESSAGING },
                            ),
                        )
                        add(
                            CommandPaletteEntry(
                                id = "android:about",
                                kind = CommandPaletteEntryKind.COMMAND,
                                label = "About Fabushi",
                                detail = "Version and platform information",
                                searchText = "About Fabushi version build Android",
                                activate = {
                                    commandPaletteOpen = false
                                    showAbout = true
                                },
                            ),
                        )
                        commandPaletteUpdateCommand(
                            state = updateState,
                            check = { updateModel.checkForUpdates(force = true) },
                            install = updateModel::downloadAndInstall,
                            openUpdates = { rendererRoute = RendererRoute.MESSAGING },
                        )?.let(::add)
                        addAll(
                            commandPaletteRootCommands(
                                activeAgentIsGroup = botState.activeBot?.isGroup,
                                activeAgentIsSharedRoom = false,
                                hasChannels = messagingState.conversations.any {
                                    it.kind == ConversationKind.CHANNEL
                                },
                                openInfoSection = {
                                    rendererRoute = RendererRoute.MESSAGING
                                },
                            ),
                        )
                    }

                    Box {
                        GrokHomeSurface(
                            accountName = state.accountName,
                            accessProjection = state.accountAccessProjection,
                            messagingState = messagingState,
                            botState = botState,
                            appAgentSurface = appAgentSurface,
                            onOpenMessaging = { rendererRoute = RendererRoute.MESSAGING },
                            onRefreshAccess = model::refreshAccountAccess,
                            onOpenAccessOnboarding = {
                                runtimePort.launchExternalUrl(ACCESS_ONBOARDING_URL)
                            },
                            onOpenAgentNetwork = { rendererRoute = RendererRoute.AGENT_NETWORK },
                            onOpenCommandPalette = {
                                commandPaletteAgentFilter = null
                                botModel.resetPaletteMessageSearch()
                                botModel.resetPaletteRoutines()
                                botModel.refreshPaletteRoutines()
                                commandPaletteOpen = true
                            },
                            onShowBotAsyncTasks = botModel::openAsyncTasks,
                            onRefreshBots = botModel::refreshBots,
                            onCreateBot = botModel::createBot,
                            onCreateGroup = botModel::createGroup,
                            onSetGroupMembers = botModel::setGroupMembers,
                            onOpenBot = { bot -> botModel.openBot(bot) },
                            onRenameBot = botModel::renameBot,
                            onUpdateBotProfile = botModel::updateBotProfile,
                            onSetBotNotifyOnUpdates = botModel::setBotNotifyOnUpdates,
                            onHideBot = botModel::hideBot,
                            onSetBotUnread = botModel::setBotUnread,
                            onDuplicateBot = botModel::duplicateBot,
                            onDeleteBot = botModel::deleteBotAndAwait,
                            onSetBotPinned = botModel::setBotPinned,
                            onMoveBotToSection = botModel::moveBotToSection,
                            onMoveBotToNewSection = botModel::moveBotToNewSection,
                            onCloseBot = botModel::closeBot,
                            onDraftChange = botModel::setDraft,
                            onSend = botModel::send,
                            onStop = botModel::stop,
                            onResolveApproval = botModel::resolveApproval,
                            onMessageTargetConsumed = botModel::consumeMessageTarget,
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
                        CommandPalette(
                            open = commandPaletteOpen,
                            entries = paletteEntries,
                            messageSearchStatus = botState.paletteMessageSearch.status,
                            routineStatus = botState.paletteRoutines.status,
                            initialTab = if (commandPaletteAgentFilter == null) {
                                CommandPaletteTab.ALL
                            } else {
                                CommandPaletteTab.ROUTINES
                            },
                            onQueryChange = botModel::setPaletteMessageQuery,
                            onDismiss = {
                                botModel.resetPaletteMessageSearch()
                                botModel.resetPaletteRoutines()
                                commandPaletteAgentFilter = null
                                commandPaletteOpen = false
                            },
                        )
                    }
                } else {
                    FabushiMessagingSurface(
                        state = state,
                        onQueryChange = model::setQuery,
                        onSearch = model::refresh,
                        onInstall = model::install,
                        onOpen = { openedMiniApp = it },
                        onApprovePermissions = model::approvePermissions,
                        onDenyPermissions = model::denyPermissions,
                        onSubmitPluginVariables = model::submitPluginVariables,
                        onCancelPluginVariables = model::cancelPluginVariables,
                        updateState = updateState,
                        onCheckUpdate = { updateModel.checkForUpdates(force = true) },
                        onInstallUpdate = updateModel::downloadAndInstall,
                        messagingState = messagingState,
                        messagingActorId = messagingModel.currentActorId,
                        onMessagingRefresh = messagingModel::refresh,
                        onCreateDirect = messagingModel::createDirect,
                        onCreateConversation = messagingModel::createConversation,
                        onSendText = { conversationId, text, replyTo, silent, scheduledAt -> messagingModel.sendText(conversationId, text, replyTo, silent, scheduledAt) },
                        onSendAttachment = messagingModel::sendAttachment,
                        onSendVoice = messagingModel::sendVoice,
                        onLoadBlob = messagingModel::loadBlob,
                        onSendContact = messagingModel::sendContact,
                        onSendPoll = messagingModel::sendPoll,
                        onVotePoll = messagingModel::votePoll,
                        onSendLocation = { conversationId, latitude, longitude -> messagingModel.sendLocation(conversationId, latitude, longitude) },
                        onEditText = messagingModel::editText,
                        onDeleteMessage = { conversationId, messageId -> messagingModel.deleteMessage(conversationId, messageId) },
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
                        authGateEnabled = true,
                        onAdvanceOnboarding = model::advanceOnboarding,
                        onSkipOnboarding = model::skipOnboarding,
                        onBeginBrowserLogin = model::beginBrowserLogin,
                        onReopenBrowserLogin = model::reopenBrowserLogin,
                        onCancelBrowserLogin = model::cancelBrowserLogin,
                        onLogout = model::logout,
                        onBackToGrokHome = { rendererRoute = RendererRoute.GROK_HOME },
                        onChatDraftChange = model::setChatDraft,
                        onSendChat = model::sendChat,
                        onStopChat = model::stopChat,
                        onMarkAssistantRead = model::markAssistantRead,
                    )
                }
            }
        
}

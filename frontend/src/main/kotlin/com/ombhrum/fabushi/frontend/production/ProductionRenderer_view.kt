package com.ombhrum.fabushi

import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidDeepLink
import com.ombhrum.fabushi.androidpreload.deeplink.AndroidPresentationDeepLink
import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointAvailabilitySnapshot
import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointId
import com.ombhrum.fabushi.androidpreload.runtime.AndroidPresentationRuntimePort
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.withContext
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
    keyboardShortcutController: GlobalKeyboardShortcutController,
    deepLinks: SharedFlow<AndroidPresentationDeepLink>,
    updateModel: AndroidUpdateViewModel,
    runtimePort: AndroidPresentationRuntimePort,
) {
            val appAgentSurface = runtimePort.appAgentSurface
            val coordinator = remember { CoordinatorClient.presentation() }

            FabushiNativeTheme {
                val model: MarketplaceViewModel = viewModel()
                val messagingModel: MessagingViewModel = viewModel()
                val botModel: MobileBotViewModel = viewModel()
                val signedInOnboardingModel: SignedInOnboardingViewModel = viewModel()
                val state by model.state.collectAsState()
                val messagingState by messagingModel.state.collectAsState()
                val botState by botModel.state.collectAsState()
                val signedInOnboardingState by signedInOnboardingModel.state.collectAsState()
                val updateState by updateModel.state.collectAsState()
                val mandatoryUpdateState by updateModel.mandatoryUpdate.collectAsState()
                val updateTrackPolicy by updateModel.trackPolicy.collectAsState()
                val autoUpdateWhenIdleOptIn by updateModel.autoUpdateWhenIdleOptIn.collectAsState()
                val autoUpdateWhenIdleGateEnabled by updateModel.autoUpdateWhenIdleGateEnabled.collectAsState()
                var openedMiniApp by remember { mutableStateOf<MarketplacePlugin?>(null) }
                var rendererRoute by remember { mutableStateOf(RendererRoute.GROK_HOME) }
                var commandPaletteOpen by remember { mutableStateOf(false) }
                var commandPaletteAgentFilter by remember { mutableStateOf<String?>(null) }
                var commandPaletteInitialTab by remember { mutableStateOf(CommandPaletteTab.ALL) }
                var showAbout by remember { mutableStateOf(false) }
                var showSettings by remember { mutableStateOf(false) }
                var computerUpdateConfirmAction by remember {
                    mutableStateOf<CommandPaletteComputerUpdateAction?>(null)
                }
                var deepLinkInfo by remember { mutableStateOf<DeepLinkInfo?>(null) }
                var shortcutNewAgentNonce by remember { mutableStateOf(0) }
                var shortcutFocusPromptNonce by remember { mutableStateOf(0) }
                var shortcutFindInChatNonce by remember { mutableStateOf(0) }
                var entrypointAvailability by remember {
                    mutableStateOf(AndroidEntrypointAvailabilitySnapshot.initial())
                }
                val rootShellNavigationOwner = remember(activity.applicationContext) {
                    createRootShellNavigationOwner(activity.applicationContext)
                }

                val coroutineScope = rememberCoroutineScope()
                val appAlertController = remember(coroutineScope) {
                    AppAlertController(coroutineScope)
                }
                DisposableEffect(appAlertController) {
                    onDispose(appAlertController::dispose)
                }
                AppAlertHost(controller = appAlertController)

                val keyboardBinding = remember(keyboardShortcutController) {
                    keyboardShortcutController.bindPresentation()
                }
                DisposableEffect(keyboardBinding) {
                    onDispose(keyboardBinding::clear)
                }

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
                MandatoryUpdateGate(
                    policy = mandatoryUpdateState,
                    updateState = updateState,
                    onCheckUpdate = { updateModel.checkForUpdates(force = true) },
                    onInstallUpdate = updateModel::downloadAndInstall,
                )
                if (isRosterPrivacyBlocked(state.accountAccessProjection)) {
                    RosterPrivacyBlockedDialog(
                        logoutBusy = state.logoutBusy,
                        logoutError = state.logoutError,
                        onSignOut = model::logout,
                        onOpenSettings = {
                            runtimePort.launchExternalUrl(ROSTER_PRIVACY_SETTINGS_URL)
                            Unit
                        },
                    )
                }
                RootConnectionResilienceHost(
                    projection = state.rootConnectionProjection,
                    onRetry = model::retryRootConnection,
                )
                RootShellNotificationHost(
                    runtimePort = runtimePort,
                    loggedIn = state.loggedIn,
                    accountGeneration = state.rootConnectionProjection.accountGeneration,
                    accountEpoch = state.rootConnectionProjection.accountEpoch,
                    accountIdentity = state.rootConnectionProjection.accountIdentity,
                    transport = state.rootConnectionProjection.transport,
                )
                RootShellStatusBadge(
                    loggedIn = state.loggedIn,
                    connection = state.rootConnectionProjection,
                )
                if (showAbout) {
                    AndroidAboutDialog(
                        updateState = updateState,
                        onClose = { showAbout = false },
                    )
                }
                if (showSettings) {
                    FabushiSettingsSurface(
                        accountLoggedIn = state.loggedIn,
                        accountName = state.accountName,
                        accountEmail = state.accountEmail,
                        accountAuthId = state.accountAuthId,
                        accountAvatarUrl = state.accountAvatarUrl,
                        accountPrivacyMode = state.accountPrivacyMode,
                        updateState = updateState,
                        updateTrackPolicy = updateTrackPolicy,
                        autoUpdateWhenIdleOptIn = autoUpdateWhenIdleOptIn,
                        autoUpdateWhenIdleGateEnabled = autoUpdateWhenIdleGateEnabled,
                        onSetAutoUpdateWhenIdleOptIn = updateModel::setAutoUpdateWhenIdleOptIn,
                        onRefreshAccountPrivacyMode = model::refreshAccountPrivacyMode,
                        onCheckUpdate = { updateModel.checkForUpdates(force = true) },
                        onInstallUpdate = updateModel::downloadAndInstall,
                        onClose = { showSettings = false },
                    )
                }
                LaunchedEffect(computerUpdateConfirmAction, appAlertController) {
                    val action = computerUpdateConfirmAction ?: return@LaunchedEffect
                    val busy = action == CommandPaletteComputerUpdateAction.BUSY_OVERRIDE
                    appAlertController.alert(
                        AppAlertRequest(
                            title = if (busy) "An agent is working" else "Update Fabushi's Computer?",
                            body = if (busy) {
                                "Waiting lets current Agent turns finish. Fabushi will update the shared computer " +
                                    "when the canonical Agent roster is idle. Files and logins are kept."
                            } else {
                                "This updates the shared computer your agents run on to the latest version. " +
                                    "Files and logins are kept. Android will not retry an update whose outcome is unknown."
                            },
                            confirmLabel = if (busy) "Update when done" else "Update Fabushi's Computer",
                            cancelLabel = if (busy) "Cancel" else "Not now",
                            secondary = if (busy) {
                                AppAlertSecondaryAction(
                                    label = "Update anyway",
                                    destructive = true,
                                    perform = {
                                        botModel.requestComputerUpdate(action = action, forceNow = true)
                                        null
                                    },
                                )
                            } else {
                                null
                            },
                            perform = {
                                botModel.requestComputerUpdate(action)
                                null
                            },
                        ),
                    ) {
                        if (computerUpdateConfirmAction == action) {
                            computerUpdateConfirmAction = null
                        }
                    }
                }

                LaunchedEffect(state.loggedIn) {
                    runtimePort.setLoggedIn(state.loggedIn)
                    if (state.loggedIn) {
                        messagingModel.refresh()
                        model.refresh()
                        botModel.refreshBots()
                        if (state.onboardingStep >= 3) signedInOnboardingModel.refresh()
                    }
                    if (!state.loggedIn) {
                        botModel.resetAccountScope()
                        signedInOnboardingModel.resetAccountScope()
                        rendererRoute = RendererRoute.GROK_HOME
                    }
                }
                LaunchedEffect(
                    state.loggedIn,
                    state.rootConnectionProjection.accountGeneration,
                    state.rootConnectionProjection.accountEpoch,
                    state.rootConnectionProjection.accountIdentity,
                    botState.activeBot?.id,
                ) {
                    rootShellNavigationOwner.observeAccount(
                        loggedIn = state.loggedIn,
                        nextAccountGeneration = state.rootConnectionProjection.accountGeneration,
                        nextAccountEpoch = state.rootConnectionProjection.accountEpoch,
                        nextAccountIdentity = state.rootConnectionProjection.accountIdentity,
                    )
                    if (state.loggedIn) {
                        rootShellNavigationOwner.observeAppliedAgent(botState.activeBot?.id)
                    }
                }

                LaunchedEffect(state.authResolved, state.loggedIn, state.onboardingStep) {
                    if (state.authResolved && state.loggedIn && state.onboardingStep >= 3) {
                        signedInOnboardingModel.refresh()
                    }
                }
                LaunchedEffect(signedInOnboardingState.active, signedInOnboardingState.createdAgentId) {
                    if (!signedInOnboardingState.active && !signedInOnboardingState.createdAgentId.isNullOrBlank()) {
                        botModel.refreshBots()
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

                val canonicalSelectedAgentId = botState.activeBot?.id?.takeIf { selectedId ->
                    botState.bots.any { it.id == selectedId }
                }
                val rosterAccessReadiness = selectRosterAccessReadiness(
                    access = state.accountAccessProjection,
                    connection = state.rootConnectionProjection,
                    agentIds = botState.bots.map { it.id },
                    selectedAgentId = canonicalSelectedAgentId,
                )

                LaunchedEffect(
                    state.loggedIn,
                    state.rootConnectionProjection.accountGeneration,
                    state.rootConnectionProjection.accountEpoch,
                    botState.bots.map { it.id },
                ) {
                    entrypointAvailability = withContext(Dispatchers.IO) {
                        coordinator.refreshEntrypointAvailability()
                    }
                }
                val agentNetworkAvailability =
                    entrypointAvailability.availability(AndroidEntrypointId.AGENT_NETWORK)
                LaunchedEffect(agentNetworkAvailability, rendererRoute) {
                    if (
                        rendererRoute == RendererRoute.AGENT_NETWORK &&
                        !agentNetworkAvailability.isOpenable
                    ) {
                        rendererRoute = RendererRoute.GROK_HOME
                    }
                }

                SideEffect {
                    fun openPalette(tab: CommandPaletteTab) {
                        commandPaletteAgentFilter = null
                        commandPaletteInitialTab = tab
                        botModel.resetPaletteMessageSearch()
                        botModel.resetPaletteFileSearch()
                        botModel.setPaletteFileQuery("")
                        botModel.resetPaletteLinks()
                        botModel.refreshPaletteLinks()
                        botModel.resetPaletteRoutines()
                        botModel.refreshPaletteRoutines()
                        botModel.refreshComputerRebuildStatus()
                        commandPaletteOpen = true
                    }
                    fun moveAgent(delta: Int) {
                        val bots = botState.bots
                        if (bots.isEmpty()) return
                        val current = bots.indexOfFirst { it.id == botState.activeBot?.id }
                        val base = if (current >= 0) current else if (delta > 0) -1 else bots.size
                        val target = (base + delta).coerceIn(0, bots.lastIndex)
                        rendererRoute = RendererRoute.GROK_HOME
                        botModel.openBot(bots[target])
                    }
                    keyboardBinding.setActions(
                        createRootShellShortcutActions(
                            handlers = GlobalShortcutHandlers(
                                newAgent = {
                                    rendererRoute = RendererRoute.GROK_HOME
                                    if (botState.activeBot != null) botModel.closeBot()
                                    shortcutNewAgentNonce += 1
                                },
                                toggleCommandPalette = {
                                    if (commandPaletteOpen) commandPaletteOpen = false
                                    else openPalette(CommandPaletteTab.ALL)
                                },
                                openSettings = {
                                    if (entrypointAvailability
                                            .availability(AndroidEntrypointId.SETTINGS)
                                            .isOpenable
                                    ) {
                                        showSettings = true
                                    }
                                },
                                openTools = { openPalette(CommandPaletteTab.ACTIONS) },
                                focusPrompt = { if (botState.activeBot != null) shortcutFocusPromptNonce += 1 },
                                openSearch = { openPalette(CommandPaletteTab.AGENTS) },
                                findInChat = if (botState.activeBot != null) {
                                    { shortcutFindInChatNonce += 1 }
                                } else null,
                                previousAgent = { moveAgent(-1) },
                                nextAgent = { moveAgent(1) },
                                navigateBack = {
                                    val available = botState.bots.mapTo(linkedSetOf()) { it.id }
                                    val targetId = rootShellNavigationOwner.navigate(
                                        RootShellNavigationDirection.BACK,
                                        available,
                                    )
                                    val target = targetId?.let { id ->
                                        botState.bots.firstOrNull { it.id == id }
                                    }
                                    if (target != null) {
                                        rendererRoute = RendererRoute.GROK_HOME
                                        botModel.openBot(target)
                                    } else if (botState.activeBot != null) {
                                        botModel.closeBot()
                                    } else if (rendererRoute != RendererRoute.GROK_HOME) {
                                        rendererRoute = RendererRoute.GROK_HOME
                                    }
                                },
                                navigateForward = {
                                    val available = botState.bots.mapTo(linkedSetOf()) { it.id }
                                    rootShellNavigationOwner.navigate(
                                        RootShellNavigationDirection.FORWARD,
                                        available,
                                    )?.let { id ->
                                        botState.bots.firstOrNull { it.id == id }?.let { bot ->
                                            rendererRoute = RendererRoute.GROK_HOME
                                            botModel.openBot(bot)
                                        }
                                    }
                                },
                                focusAgent = { index ->
                                    botState.bots.getOrNull(index - 1)?.let { bot ->
                                        rendererRoute = RendererRoute.GROK_HOME
                                        botModel.openBot(bot)
                                    }
                                },
                                toggleSidebar = {
                                    rendererRoute = if (rendererRoute == RendererRoute.AGENT_NETWORK) {
                                        RendererRoute.GROK_HOME
                                    } else if (agentNetworkAvailability.isOpenable) {
                                        RendererRoute.AGENT_NETWORK
                                    } else {
                                        RendererRoute.GROK_HOME
                                    }
                                },
                            ),
                            agentCount = botState.bots.size,
                        ),
                    )
                    val rootOverlayClose: (() -> Unit)? = when {
                        commandPaletteOpen -> ({ commandPaletteOpen = false })
                        showSettings -> ({ showSettings = false })
                        showAbout -> ({ showAbout = false })
                        openedMiniApp != null -> ({ openedMiniApp = null })
                        else -> null
                    }
                    keyboardBinding.acceptOverlayState(
                        RootOverlayEscapeState(
                            isArmed = rootOverlayClose != null,
                            isOverlayStacked = appAlertController.getSnapshot() != null ||
                                computerUpdateConfirmAction != null || deepLinkInfo != null,
                            allowPreventedRootSearchEscape = commandPaletteOpen,
                            close = rootOverlayClose ?: {},
                        ),
                    )
                }

                val active = openedMiniApp
                if (active != null) {
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
                } else if (
                    state.onboardingStep >= 3 &&
                    state.authResolved &&
                    state.loggedIn &&
                    signedInOnboardingState.active
                ) {
                    SignedInOnboardingSurface(
                        state = signedInOnboardingState,
                        onNext = signedInOnboardingModel::next,
                        onBack = signedInOnboardingModel::back,
                        onNameChange = { signedInOnboardingModel.updateDraft(name = it) },
                        onDescriptionChange = { signedInOnboardingModel.updateDraft(description = it) },
                        onToggleTool = signedInOnboardingModel::toggleDailyTool,
                        onChooseSuggestion = signedInOnboardingModel::chooseSuggestion,
                        onChooseShape = { signedInOnboardingModel.choosePersona(shape = it) },
                        onChooseColor = { signedInOnboardingModel.choosePersona(color = it) },
                        onCreate = signedInOnboardingModel::createFirstAgent,
                    )
                } else if (
                    state.onboardingStep >= 3 &&
                    state.authResolved &&
                    state.loggedIn &&
                    rendererRoute == RendererRoute.AGENT_NETWORK &&
                    agentNetworkAvailability.isOpenable
                ) {
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
                    val paletteFileEntries = commandPaletteFileEntries(
                        files = botState.paletteFileSearch.value.filter { file ->
                            commandPaletteAgentFilter == null || file.agentId == commandPaletteAgentFilter
                        },
                        agentNames = paletteAgentNames,
                    ) { file ->
                        val targetBot = if (file.agentId == "mahayana-assistant") {
                            MobileBotSummaryAndroid(
                                id = "mahayana-assistant",
                                name = "Mahayana",
                                description = "Mahayana multi-step agent",
                            )
                        } else {
                            botState.bots.firstOrNull { it.id == file.agentId }
                        }
                        if (targetBot != null) {
                            rendererRoute = RendererRoute.GROK_HOME
                            botModel.openBot(targetBot, targetEntryId = file.entryId)
                        }
                    }
                    val paletteLinkEntries = commandPaletteLinkEntries(
                        links = botState.paletteLinks.value,
                        metadata = botState.paletteLinks.metadata,
                    ) { link ->
                        runtimePort.launchExternalUrl(link.url)
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
                    val computerUpdateAction = commandPaletteComputerUpdateAction(
                        activeAgentIsGroup = botState.activeBot?.isGroup,
                        workingAgentCount = botState.bots.count { it.isRunning } +
                            if (botState.busy) 1 else 0,
                        snapshot = botState.computerRebuild.takeUnless { it.loading }?.let {
                            CommandPaletteComputerUpdateSnapshot(
                                imageUpdateAvailable = it.imageUpdateAvailable,
                                pending = it.pending,
                                outcomeUnknown = it.outcomeUnknown,
                                rebuildKind = it.kind,
                                queuedWhenIdle = it.queuedWhenIdle,
                            )
                        },
                    )
                    val paletteEntries = buildList {
                        addAll(paletteMessageEntries)
                        addAll(paletteFileEntries)
                        addAll(paletteLinkEntries)
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
                                id = "android:settings",
                                kind = CommandPaletteEntryKind.COMMAND,
                                label = "Settings",
                                detail = "Appearance and app updates",
                                searchText = "Settings preferences appearance theme updates",
                                activate = {
                                    commandPaletteOpen = false
                                    if (entrypointAvailability
                                            .availability(AndroidEntrypointId.SETTINGS)
                                            .isOpenable
                                    ) {
                                        showSettings = true
                                    }
                                },
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
                                computerUpdateAction = computerUpdateAction,
                                openComputerUpdateConfirm = { action ->
                                    commandPaletteOpen = false
                                    computerUpdateConfirmAction = action
                                },
                            ),
                        )
                    }

                    Box {
                        GrokHomeSurface(
                            accountName = state.accountName,
                            accountAuthId = state.accountAuthId,
                            accountAvatarUrl = state.accountAvatarUrl,
                            accessProjection = state.accountAccessProjection,
                            rosterAccessReadiness = rosterAccessReadiness,
                            messagingState = messagingState,
                            botState = botState,
                            appAgentSurface = appAgentSurface,
                            agentNetworkAvailability = agentNetworkAvailability,
                            hiddenChatsAvailability =
                                entrypointAvailability.availability(AndroidEntrypointId.HIDDEN_CHATS),
                            onOpenMessaging = { rendererRoute = RendererRoute.MESSAGING },
                            onRefreshAccess = model::refreshAccountAccess,
                            onOpenAccessOnboarding = {
                                runtimePort.launchExternalUrl(ACCESS_ONBOARDING_URL)
                            },
                            onOpenAgentNetwork = {
                                if (agentNetworkAvailability.isOpenable) {
                                    rendererRoute = RendererRoute.AGENT_NETWORK
                                }
                            },
                            onOpenConversationOutline = botModel::openConversationOutline,
                            onCloseConversationOutline = botModel::closeConversationOutline,
                            onSelectConversationOutlineAgent = botModel::selectConversationOutlineAgent,
                            onRefreshConversationOutline = botModel::refreshConversationOutline,
                            onOpenCommandPalette = {
                                commandPaletteAgentFilter = null
                                commandPaletteInitialTab = CommandPaletteTab.ALL
                                botModel.resetPaletteMessageSearch()
                                botModel.resetPaletteFileSearch()
                                botModel.setPaletteFileQuery("")
                                botModel.resetPaletteLinks()
                                botModel.refreshPaletteLinks()
                                botModel.resetPaletteRoutines()
                                botModel.refreshPaletteRoutines()
                                botModel.refreshComputerRebuildStatus()
                                commandPaletteOpen = true
                            },
                            onShowBotAsyncTasks = botModel::openAsyncTasks,
                            onRefreshBots = botModel::refreshBots,
                            onCreateBot = botModel::createBot,
                            onCreateGroup = botModel::createGroup,
                            onSetGroupMembers = botModel::setGroupMembers,
                            onOpenBot = { bot -> botModel.openBot(bot) },
                            onRenameBot = botModel::renameBot,
                            onBeginAgentSettings = botModel::beginAgentSettings,
                            onEndAgentSettings = botModel::endAgentSettings,
                            onUpdateBotProfile = botModel::updateBotProfile,
                            onSetBotNotifyOnUpdates = botModel::setBotNotifyOnUpdates,
                            onHideBot = botModel::hideBot,
                            onUnhideBot = botModel::unhideBot,
                            onSetBotUnread = botModel::setBotUnread,
                            onDuplicateBot = botModel::duplicateBot,
                            onDeleteBot = botModel::deleteBotAndAwait,
                            onSetBotPinned = botModel::setBotPinned,
                            onReorderPinnedBot = botModel::reorderPinnedBot,
                            onToggleSidebarSection = botModel::toggleSidebarSection,
                            onRenameSidebarSection = botModel::renameSidebarSection,
                            onDeleteSidebarSection = botModel::deleteSidebarSection,
                            onMoveSidebarSection = botModel::moveSidebarSection,
                            onMoveBotToSection = botModel::moveBotToSection,
                            onMoveBotsToSection = botModel::moveBotsToSection,
                            onMoveBotToNewSection = botModel::moveBotToNewSection,
                            onCloseBot = botModel::closeBot,
                            onDraftChange = botModel::setDraft,
                            onAddDraftAttachments = botModel::addDraftAttachments,
                            onRemoveDraftAttachment = botModel::removeDraftAttachment,
                            onSend = botModel::send,
                            onRetrySend = botModel::retrySend,
                            onCancelQueuedSend = botModel::cancelQueuedSend,
                            onRetryTranscriptLoad = botModel::retryTranscriptLoad,
                            onLoadOlderMessages = botModel::loadOlderMessages,
                            onRespondToWidget = botModel::respondToWidget,
                            onDismissWidget = botModel::dismissWidget,
                            onStop = botModel::stop,
                            onResolveApproval = botModel::resolveApproval,
                            onMessageTargetConsumed = botModel::consumeMessageTarget,
                            shortcutNewAgentNonce = shortcutNewAgentNonce,
                            shortcutFocusPromptNonce = shortcutFocusPromptNonce,
                            shortcutFindInChatNonce = shortcutFindInChatNonce,
                            onPromptEditableFocusChanged = keyboardBinding::setEditableFocused,
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
                            fileSearchStatus = botState.paletteFileSearch.status,
                            linkSearchStatus = botState.paletteLinks.status,
                            routineStatus = botState.paletteRoutines.status,
                            initialTab = if (commandPaletteAgentFilter == null) {
                                commandPaletteInitialTab
                            } else {
                                CommandPaletteTab.ROUTINES
                            },
                            onQueryChange = { query ->
                                botModel.setPaletteMessageQuery(query)
                                botModel.setPaletteFileQuery(query)
                            },
                            onDismiss = {
                                botModel.resetPaletteMessageSearch()
                                botModel.resetPaletteFileSearch()
                                botModel.resetPaletteLinks()
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
                        entrypointAvailability = entrypointAvailability,
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

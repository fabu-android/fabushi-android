package com.ombhrum.fabushi

import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
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
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAutoReviewInstructions
import com.ombhrum.fabushi.androidpreload.runtime.AndroidAutoReviewInstructionsSnapshot
import com.ombhrum.fabushi.androidpreload.runtime.AndroidTimeZoneSettings
import com.ombhrum.fabushi.androidpreload.runtime.AndroidEntrypointId
import com.ombhrum.fabushi.androidpreload.runtime.AndroidPresentationRuntimePort
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.util.UUID

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
                var showFeedback by rememberSaveable { mutableStateOf(false) }
                var autoReviewSnapshot by remember {
                    mutableStateOf<AndroidAutoReviewInstructionsSnapshot?>(null)
                }
                var autoReviewBusy by remember { mutableStateOf(false) }
                var autoReviewError by remember { mutableStateOf<String?>(null) }
                var autoReviewRequestGeneration by remember { mutableStateOf(0L) }
                var timeZoneSettings by remember { mutableStateOf<AndroidTimeZoneSettings?>(null) }
                var timeZoneBusy by remember { mutableStateOf(false) }
                var timeZoneError by remember { mutableStateOf<String?>(null) }
                var timeZoneRequestGeneration by remember { mutableStateOf(0L) }
                var localToolPermissionSettings by remember {
                    mutableStateOf<AndroidLocalToolPermissionSettings?>(null)
                }
                var localToolPermissionBusy by remember { mutableStateOf(false) }
                var localToolPermissionError by remember { mutableStateOf<String?>(null) }
                var localToolPermissionRequestGeneration by remember { mutableStateOf(0L) }
                var processMetricsLocalEnabled by remember { mutableStateOf(false) }
                var processMetricsBusy by remember { mutableStateOf(false) }
                var processMetricsError by remember { mutableStateOf<String?>(null) }
                var processMetricsRequestGeneration by remember { mutableStateOf(0L) }
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
                            if (link is AndroidDeepLink.PluginAdd || link is AndroidDeepLink.Open) {
                                rendererRoute = RendererRoute.MESSAGING
                            }
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
                if (showFeedback) {
                    val feedbackAccountScope =
                        state.rootConnectionProjection.accountGeneration.toString() + ":" +
                            state.rootConnectionProjection.accountEpoch.toString() + ":" +
                            state.rootConnectionProjection.accountIdentity
                    FabushiFeedbackSurface(
                        accountScopeKey = feedbackAccountScope,
                        conversationId = null,
                        submit = coordinator::submitFeedback,
                        onClose = { showFeedback = false },
                    )
                }
                fun refreshTimeZoneSettings() {
                    val generation = timeZoneRequestGeneration + 1L
                    timeZoneRequestGeneration = generation
                    timeZoneBusy = true
                    timeZoneError = null
                    coroutineScope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching { coordinator.timeZoneSettings() }
                        }
                        if (generation != timeZoneRequestGeneration) return@launch
                        timeZoneBusy = false
                        result.onSuccess { timeZoneSettings = it }
                            .onFailure { timeZoneError = it.message ?: "Time zone settings unavailable" }
                    }
                }

                fun saveTimeZoneOverride(value: String?) {
                    val generation = timeZoneRequestGeneration + 1L
                    timeZoneRequestGeneration = generation
                    timeZoneBusy = true
                    timeZoneError = null
                    coroutineScope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching { coordinator.setTimeZoneOverride(value) }
                        }
                        if (generation != timeZoneRequestGeneration) return@launch
                        timeZoneBusy = false
                        result.onSuccess { timeZoneSettings = it }
                            .onFailure { timeZoneError = it.message ?: "Time zone setting was not saved" }
                    }
                }

                fun refreshLocalToolPermissionSettings() {
                    val generation = localToolPermissionRequestGeneration + 1L
                    localToolPermissionRequestGeneration = generation
                    localToolPermissionBusy = true
                    localToolPermissionError = null
                    coroutineScope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching { coordinator.localToolPermissionSettings() }
                        }
                        if (generation != localToolPermissionRequestGeneration) return@launch
                        localToolPermissionBusy = false
                        result.onSuccess { localToolPermissionSettings = it }
                            .onFailure {
                                localToolPermissionError =
                                    it.message ?: "Computer tool permission settings unavailable"
                            }
                    }
                }

                fun saveLocalToolPermission(value: AndroidLocalToolPermission) {
                    val generation = localToolPermissionRequestGeneration + 1L
                    localToolPermissionRequestGeneration = generation
                    localToolPermissionBusy = true
                    localToolPermissionError = null
                    coroutineScope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching { coordinator.setLocalToolPermission(value) }
                        }
                        if (generation != localToolPermissionRequestGeneration) return@launch
                        localToolPermissionBusy = false
                        result.onSuccess { localToolPermissionSettings = it }
                            .onFailure {
                                localToolPermissionError =
                                    it.message ?: "Computer tool permission was not saved"
                            }
                    }
                }

                fun refreshProcessMetricsSettings() {
                    val generation = processMetricsRequestGeneration + 1L
                    processMetricsRequestGeneration = generation
                    processMetricsBusy = true
                    processMetricsError = null
                    coroutineScope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching { coordinator.processMetricsSettings() }
                        }
                        if (generation != processMetricsRequestGeneration) return@launch
                        processMetricsBusy = false
                        result.onSuccess {
                            processMetricsLocalEnabled = it.optBoolean("localEnabled", false)
                        }.onFailure {
                            processMetricsError = it.message ?: "Performance diagnostics settings unavailable"
                        }
                    }
                }

                fun saveProcessMetricsLocalEnabled(value: Boolean) {
                    val generation = processMetricsRequestGeneration + 1L
                    processMetricsRequestGeneration = generation
                    processMetricsBusy = true
                    processMetricsError = null
                    coroutineScope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching { coordinator.setProcessMetricsLocalEnabled(value) }
                        }
                        if (generation != processMetricsRequestGeneration) return@launch
                        processMetricsBusy = false
                        result.onSuccess {
                            processMetricsLocalEnabled = it.optBoolean("localEnabled", false)
                        }.onFailure {
                            processMetricsError = it.message ?: "Performance diagnostics setting was not saved"
                        }
                    }
                }

                fun refreshAutoReviewInstructions() {
                    val generation = autoReviewRequestGeneration + 1L
                    autoReviewRequestGeneration = generation
                    autoReviewBusy = true
                    autoReviewError = null
                    coroutineScope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching { coordinator.autoReviewInstructions() }
                        }
                        if (generation != autoReviewRequestGeneration) return@launch
                        autoReviewBusy = false
                        result.onSuccess { autoReviewSnapshot = it }
                            .onFailure { autoReviewError = it.message ?: "Auto Review settings unavailable" }
                    }
                }

                fun saveAutoReviewInstructions(
                    expectedRevision: Long,
                    isEnabled: Boolean,
                    allowInstructions: List<String>,
                    blockInstructions: List<String>,
                ) {
                    val generation = autoReviewRequestGeneration + 1L
                    autoReviewRequestGeneration = generation
                    autoReviewBusy = true
                    autoReviewError = null
                    coroutineScope.launch {
                        val result = withContext(Dispatchers.IO) {
                            runCatching {
                                coordinator.setAutoReviewInstructions(
                                    expectedRevision = expectedRevision,
                                    instructions = AndroidAutoReviewInstructions(
                                        isEnabled = isEnabled,
                                        allowInstructions = allowInstructions,
                                        blockInstructions = blockInstructions,
                                    ),
                                )
                            }
                        }
                        if (generation != autoReviewRequestGeneration) return@launch
                        autoReviewBusy = false
                        result.onSuccess { autoReviewSnapshot = it }
                            .onFailure { autoReviewError = it.message ?: "Auto Review settings were not saved" }
                    }
                }

                LaunchedEffect(
                    showSettings,
                    state.loggedIn,
                    state.rootConnectionProjection.accountEpoch,
                    state.rootConnectionProjection.accountIdentity,
                ) {
                    autoReviewRequestGeneration += 1L
                    autoReviewSnapshot = null
                    autoReviewError = null
                    autoReviewBusy = false
                    timeZoneRequestGeneration += 1L
                    timeZoneSettings = null
                    timeZoneError = null
                    timeZoneBusy = false
                    localToolPermissionRequestGeneration += 1L
                    localToolPermissionSettings = null
                    localToolPermissionError = null
                    localToolPermissionBusy = false
                    processMetricsRequestGeneration += 1L
                    processMetricsLocalEnabled = false
                    processMetricsError = null
                    processMetricsBusy = false
                    if (showSettings) {
                        refreshTimeZoneSettings()
                        refreshLocalToolPermissionSettings()
                        refreshProcessMetricsSettings()
                    }
                    if (showSettings && state.loggedIn) {
                        refreshAutoReviewInstructions()
                    }
                }

                if (showSettings) {
                    FabushiSettingsSurface(
                        accountLoggedIn = state.loggedIn,
                        accountName = state.accountName,
                        accountEmail = state.accountEmail,
                        accountAuthId = state.accountAuthId,
                        accountAvatarUrl = state.accountAvatarUrl,
                        accountProfileBusy = state.accountProfileBusy,
                        accountProfileError = state.accountProfileError,
                        accountPrivacyMode = state.accountPrivacyMode,
                        accountUsageSummary = state.accountUsageSummary,
                        accountUsageBusy = state.accountUsageBusy,
                        accountUsageError = state.accountUsageError,
                        timeZoneSettings = timeZoneSettings,
                        timeZoneBusy = timeZoneBusy,
                        timeZoneError = timeZoneError,
                        onSaveTimeZoneOverride = ::saveTimeZoneOverride,
                        localToolPermissionSettings = localToolPermissionSettings,
                        localToolPermissionBusy = localToolPermissionBusy,
                        localToolPermissionError = localToolPermissionError,
                        onSetLocalToolPermission = ::saveLocalToolPermission,
                        processMetricsLocalEnabled = processMetricsLocalEnabled,
                        processMetricsBusy = processMetricsBusy,
                        processMetricsError = processMetricsError,
                        onSetProcessMetricsLocalEnabled = ::saveProcessMetricsLocalEnabled,
                        autoReviewSnapshot = autoReviewSnapshot,
                        autoReviewBusy = autoReviewBusy,
                        autoReviewError = autoReviewError,
                        onRefreshAutoReviewInstructions = ::refreshAutoReviewInstructions,
                        onSaveAutoReviewInstructions = ::saveAutoReviewInstructions,
                        credentialVaultItems = state.credentialVaultItems,
                        credentialVaultBusy = state.credentialVaultBusy,
                        credentialVaultError = state.credentialVaultError,
                        onRefreshCredentialVault = model::refreshCredentialVault,
                        onSaveCredential = model::saveCredential,
                        onRevokeCredential = model::revokeCredential,
                        onUpdateAccountDisplayName = model::updateAccountDisplayName,
                        updateState = updateState,
                        updateTrackPolicy = updateTrackPolicy,
                        autoUpdateWhenIdleOptIn = autoUpdateWhenIdleOptIn,
                        autoUpdateWhenIdleGateEnabled = autoUpdateWhenIdleGateEnabled,
                        onSetAutoUpdateWhenIdleOptIn = updateModel::setAutoUpdateWhenIdleOptIn,
                        onRefreshAccountPrivacyMode = model::refreshAccountPrivacyMode,
                        onRefreshAccountUsage = model::refreshAccountUsageSummary,
                        onCancelSandTrial = model::cancelSandTrial,
                        onAccountUpgradeCta = { cta ->
                            when (cta.kind) {
                                "open-url" -> cta.url?.let(runtimePort::launchExternalUrl)
                                "dashboard-action" -> model.performAccountUpgradeCta(cta)
                            }
                        },
                        onCheckUpdate = { updateModel.checkForUpdates(force = true) },
                        onInstallUpdate = updateModel::downloadAndInstall,
                        onOpenFeedback = {
                            showSettings = false
                            showFeedback = true
                        },
                        onOpenHelpCenter = {
                            runtimePort.launchExternalUrl(FABUSHI_HELP_CENTER_URL)
                            Unit
                        },
                        onOpenAbout = {
                            showSettings = false
                            showAbout = true
                        },
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
                LaunchedEffect(
                    state.authResolved,
                    state.loggedIn,
                    state.onboardingStep,
                    signedInOnboardingState.active,
                ) {
                    if (!state.authResolved) return@LaunchedEffect
                    val signinGate = when {
                        !state.loggedIn -> "sign-in"
                        state.onboardingStep < 3 || signedInOnboardingState.active -> "onboarding"
                        else -> null
                    }
                    if (signinGate != null) {
                        withContext(Dispatchers.IO) {
                            runCatching { coordinator.reportSigninGate(gate = signinGate) }
                        }
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
                        showFeedback -> ({ showFeedback = false })
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
                            onSetBotAvatarImage = botModel::updateBotAvatarImage,
                            onGenerateBotAvatarImage = botModel::generateBotAvatarImage,
                            onStagePickedBotAvatarImage = botModel::stagePickedBotAvatarImage,
                            onStageGeneratedBotAvatarImage = botModel::stageGeneratedBotAvatarImage,
                            onEncodeBotAvatarCrop = botModel::encodeBotAvatarCrop,
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
                        onSaveMcpServerInstructions = model::saveMcpServerInstructions,
                        onLogoutMcpAccount = model::logoutMcpAccount,
                        onRenameMcpAccount = model::renameMcpAccount,
                        onRemoveMcpAccount = model::removeMcpAccount,
                        onOpenMcpServerTools = model::openMcpServerTools,
                        onRetryMcpServerTools = model::retryMcpServerTools,
                        onToggleMcpServerTool = model::toggleMcpServerTool,
                        updateState = updateState,
                        onCheckUpdate = { updateModel.checkForUpdates(force = true) },
                        onInstallUpdate = updateModel::downloadAndInstall,
                        messagingState = messagingState,
                        messagingActorId = messagingModel.currentActorId,
                        onMessagingRefresh = messagingModel::refresh,
                        onHumanCallsRefresh = messagingModel::refreshHumanCalls,
                        onCreateDirect = messagingModel::createDirect,
                        onCreateConversation = messagingModel::createConversation,
                        onSendText = { conversationId, text, replyTo, silent, scheduledAt -> messagingModel.sendText(conversationId, text, replyTo, silent, scheduledAt) },
                        onSendAttachment = messagingModel::sendAttachment,
                        onSendVoice = messagingModel::sendVoice,
                        onLoadBlob = messagingModel::loadBlob,
                        onLoadBlobFile = messagingModel::loadBlobToFile,
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


@Composable
internal fun FabushiFeedbackSurface(
    accountScopeKey: String,
    conversationId: String?,
    submit: suspend (message: String, submissionId: String, conversationId: String?) -> JSONObject,
    onClose: () -> Unit,
) {
    var message by rememberSaveable(accountScopeKey) { mutableStateOf("") }
    var includeConversation by rememberSaveable(accountScopeKey, conversationId) {
        // Desktop defaults this opt-in off unless an explicit caller override says otherwise.
        mutableStateOf(false)
    }
    var phaseName by rememberSaveable(accountScopeKey) { mutableStateOf(FeedbackPhase.IDLE.name) }
    var failureCode by rememberSaveable(accountScopeKey) { mutableStateOf<String?>(null) }
    var submissionId by rememberSaveable(accountScopeKey) { mutableStateOf<String?>(null) }
    var restorationChecked by remember(accountScopeKey) { mutableStateOf(false) }
    val latestAccountScopeKey by rememberUpdatedState(accountScopeKey)
    val scope = rememberCoroutineScope()
    val phase = feedbackPhaseFromSaved(phaseName)
    val sending = phase == FeedbackPhase.SENDING
    val sent = phase == FeedbackPhase.SENT
    val cleanMessage = message.trim()
    val canSend = feedbackCanSend(message, phase)

    LaunchedEffect(accountScopeKey) {
        if (!restorationChecked) {
            phaseName = feedbackPhaseAfterProcessRestore(phaseName).name
            restorationChecked = true
        }
    }

    fun setPhase(next: FeedbackPhase) {
        phaseName = next.name
    }

    fun closeIfAllowed() {
        if (feedbackCanDismiss(phase)) onClose()
    }

    fun sendExplicitly() {
        if (!canSend) return
        val capturedScope = accountScopeKey
        val capturedMessage = cleanMessage
        val capturedConversation = conversationId?.takeIf { includeConversation }
        val stableSubmissionId = submissionId ?: UUID.randomUUID().toString().also {
            submissionId = it
        }
        setPhase(FeedbackPhase.SENDING)
        failureCode = null
        scope.launch {
            val outcome = withContext(Dispatchers.IO) {
                runCatching {
                    submit(capturedMessage, stableSubmissionId, capturedConversation)
                }
            }
            if (capturedScope != latestAccountScopeKey) return@launch
            outcome.fold(
                onSuccess = { response ->
                    if (response.optBoolean("ok", false)) {
                        setPhase(FeedbackPhase.SENT)
                        failureCode = null
                    } else {
                        setPhase(FeedbackPhase.FAILED)
                        failureCode = response.optString("code").ifBlank { "unavailable" }
                    }
                },
                onFailure = {
                    setPhase(FeedbackPhase.FAILED)
                    failureCode = "unavailable"
                },
            )
        }
    }

    AlertDialog(
        onDismissRequest = ::closeIfAllowed,
        modifier = Modifier.testTag("fabushi-feedback-dialog"),
        title = { Text(if (sent) "Feedback sent" else "Send feedback") },
        text = {
            if (sent) {
                Text(
                    "Thank you. Your feedback was sent.",
                    modifier = Modifier.testTag("feedback-sent"),
                )
            } else {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("Tell us what happened or what would make Fabushi better.")
                    OutlinedTextField(
                        value = message,
                        onValueChange = { value ->
                            if (!sending) {
                                message = value.take(FEEDBACK_MESSAGE_MAX_CHARS + 1)
                                if (feedbackRetryKeepsSubmissionIdentity(phase)) {
                                    setPhase(FeedbackPhase.IDLE)
                                    failureCode = null
                                    submissionId = null
                                }
                            }
                        },
                        enabled = !sending,
                        label = { Text("Feedback") },
                        supportingText = {
                            Text("${message.length} / $FEEDBACK_MESSAGE_MAX_CHARS")
                        },
                        modifier = Modifier.testTag("feedback-message"),
                    )
                    if (conversationId != null) {
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Checkbox(
                                checked = includeConversation,
                                onCheckedChange = { checked ->
                                    if (!sending) includeConversation = checked
                                },
                                enabled = !sending,
                                modifier = Modifier.testTag("feedback-include-conversation"),
                            )
                            Text("Include current conversation ID")
                        }
                    }
                    if (message.length > FEEDBACK_MESSAGE_MAX_CHARS) {
                        Text(
                            "Feedback must be $FEEDBACK_MESSAGE_MAX_CHARS characters or fewer.",
                            color = MaterialTheme.colorScheme.error,
                        )
                    }
                    if (phase == FeedbackPhase.FAILED) {
                        Text(
                            feedbackErrorMessage(failureCode),
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.testTag("feedback-error"),
                        )
                    }
                    if (phase == FeedbackPhase.OUTCOME_UNKNOWN) {
                        Text(
                            feedbackOutcomeUnknownMessage(),
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.testTag("feedback-outcome-unknown"),
                        )
                    }
                }
            }
        },
        confirmButton = {
            if (sent) {
                TextButton(
                    onClick = onClose,
                    modifier = Modifier.testTag("feedback-done"),
                ) { Text("Done") }
            } else {
                TextButton(
                    onClick = ::sendExplicitly,
                    enabled = canSend,
                    modifier = Modifier.testTag("feedback-send"),
                ) {
                    Text(
                        when {
                            sending -> "Sending…"
                            feedbackRetryKeepsSubmissionIdentity(phase) -> "Retry"
                            else -> "Send"
                        },
                    )
                }
            }
        },
        dismissButton = {
            if (!sent) {
                TextButton(
                    onClick = ::closeIfAllowed,
                    enabled = feedbackCanDismiss(phase),
                    modifier = Modifier.testTag("feedback-close"),
                ) { Text("Close") }
            }
        },
    )
}
